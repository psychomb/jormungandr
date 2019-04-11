use chain_addr::{Address, AddressReadable};
use chain_impl_mockchain::{
    transaction::{
        AuthenticatedTransaction, Balance, Input, InputEnum, InputType, NoExtra, Output,
        Transaction, TransactionId, Witness,
    },
    value::{Value, ValueError},
};
use jcli_app::{transaction::common, utils::io};
use std::{collections::HashMap, io::Write, path::PathBuf};
use strfmt::{strfmt, FmtError};
use structopt::StructOpt;

custom_error! {pub InfoError
    Io { source: std::io::Error } = "I/O Error",
    FormatError { source: FmtError } = "Invalid format",
    ReadTransaction { source: common::CommonError } = "cannot read the transaction",
    ValueError { source: ValueError } = "Invalid values",
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub struct Info {
    #[structopt(flatten)]
    pub common: common::CommonTransaction,

    #[structopt(flatten)]
    pub fee: common::CommonFees,

    /// write the info in the given file or print it to the standard output
    pub output: Option<PathBuf>,

    /// formatting for the output to displays
    /// user "{name}" to display the variable with the named `name'.
    ///
    /// available variables: id, num_inputs, num_outputs, num_witnesses, fee
    /// balance, input, output
    ///
    #[structopt(
        long = "format",
        default_value = "Transaction `{id}'\n  Input:   {input}\n  Output:  {output}\n  Fees:    {fee}\n  Balance: {balance}\n"
    )]
    pub format: String,

    /// display only the inputs of type UTxO
    pub only_utxos: bool,
    /// display only the inputs of type Account
    pub only_accounts: bool,
    /// display only the outputs
    pub only_outputs: bool,

    /// formatting for the UTxO inputs of the transaction. This format
    /// will be applied to every inputs of type UTxO.
    ///
    /// available variables: txid, index and value.
    ///
    #[structopt(alias = "utxo", default_value = " - {txid}:{index} {value}")]
    pub format_utxo_input: String,

    /// formatting for the Account inputs of the transaction. This format
    /// will be applied to every inputs of type account.
    ///
    /// available variables: account and value.
    ///
    #[structopt(alias = "account", default_value = " - {account} {value}")]
    pub format_account_input: String,

    /// Display the outputs of the transaction, this function will be called
    /// for every outputs of the transaction
    ///
    /// available variables: address and value.
    #[structopt(alias = "output", default_value = " + {address} {value}")]
    pub format_output: String,
}

impl Info {
    pub fn exec(self) -> Result<(), InfoError> {
        let transaction = self
            .common
            .load_transaction()
            .map(OneOrTheOther::Tx)
            .or_else(|_| {
                self.common
                    .load_auth_transaction()
                    .map(OneOrTheOther::TxAux)
            })?;

        let mut output = io::open_file_write(&self.output);

        self.display_info(&mut output, &transaction)?;
        self.display_inputs(&mut output, transaction.inputs())?;

        if !self.only_accounts || !self.only_utxos {
            self.display_outputs(&mut output, transaction.outputs())?;
        }
        Ok(())
    }

    fn display_outputs<W: Write>(
        &self,
        mut writer: W,
        outputs: &[Output<Address>],
    ) -> Result<(), InfoError> {
        for output in outputs {
            self.display_output(&mut writer, output)?;
        }
        Ok(())
    }

    fn display_inputs<W: Write>(&self, mut writer: W, inputs: &[Input]) -> Result<(), InfoError> {
        for input in inputs {
            match input.get_type() {
                InputType::Account => {
                    if self.only_outputs || self.only_utxos {
                        continue;
                    }
                }
                InputType::Utxo => {
                    if self.only_outputs || self.only_accounts {
                        continue;
                    }
                }
            }
            self.display_input(&mut writer, input.to_enum())?;
        }
        Ok(())
    }

    fn display_output<W: Write>(
        &self,
        mut writer: W,
        output: &Output<Address>,
    ) -> Result<(), InfoError> {
        let mut vars = HashMap::new();

        vars.insert(
            "address".to_owned(),
            AddressReadable::from_address(&output.address).to_string(),
        );
        vars.insert("value".to_owned(), output.value.0.to_string());

        let formatted = strfmt(&self.format, &vars)?;
        write!(writer, "{}", formatted)?;
        Ok(())
    }

    fn display_input<W: Write>(&self, mut writer: W, input: InputEnum) -> Result<(), InfoError> {
        let mut vars = HashMap::new();

        match input {
            InputEnum::UtxoInput(utxo_ptr) => {
                vars.insert("txid".to_owned(), utxo_ptr.transaction_id.to_string());
                vars.insert("index".to_owned(), utxo_ptr.output_index.to_string());
                vars.insert("value".to_owned(), utxo_ptr.value.0.to_string());
            }
            InputEnum::AccountInput(account, value) => {
                let account: chain_crypto::PublicKey<_> = account.into();
                vars.insert("account".to_owned(), account.to_string());
                vars.insert("value".to_owned(), value.0.to_string());
            }
        }

        let formatted = strfmt(&self.format, &vars)?;
        write!(writer, "{}", formatted)?;
        Ok(())
    }

    fn display_info<W: Write>(
        &self,
        mut writer: W,
        transaction: &OneOrTheOther,
    ) -> Result<(), InfoError> {
        let mut vars = HashMap::new();

        let fee_algo = self.fee.linear_fee();
        let builder = transaction.builder();

        vars.insert("id".to_owned(), transaction.id().to_string());
        vars.insert(
            "num_inputs".to_owned(),
            transaction.inputs().len().to_string(),
        );
        vars.insert(
            "num_outputs".to_owned(),
            transaction.outputs().len().to_string(),
        );
        vars.insert(
            "num_witnesses".to_owned(),
            transaction.witnesses().len().to_string(),
        );
        vars.insert("input".to_owned(), transaction.total_input()?.0.to_string());
        vars.insert(
            "output".to_owned(),
            transaction.total_output()?.0.to_string(),
        );
        vars.insert(
            "fee".to_owned(),
            builder.estimate_fee(&fee_algo)?.0.to_string(),
        );
        vars.insert(
            "balance".to_owned(),
            match builder.get_balance(fee_algo)? {
                Balance::Negative(value) => format!("-{}", value.0),
                Balance::Positive(value) => format!("+{}", value.0),
                Balance::Zero => "0".to_string(),
            },
        );

        let formatted = strfmt(&self.format, &vars)?;

        write!(writer, "{}", formatted)?;
        Ok(())
    }
}

enum OneOrTheOther {
    Tx(Transaction<Address, NoExtra>),
    TxAux(AuthenticatedTransaction<Address, NoExtra>),
}

use chain_impl_mockchain::txbuilder::TransactionBuilder;

impl OneOrTheOther {
    fn builder(&self) -> TransactionBuilder<Address, NoExtra> {
        self.transaction().clone().into()
    }
    fn transaction(&self) -> &Transaction<Address, NoExtra> {
        match self {
            OneOrTheOther::Tx(tx) => tx,
            OneOrTheOther::TxAux(txaux) => &txaux.transaction,
        }
    }
    fn id(&self) -> TransactionId {
        self.transaction().hash()
    }
    fn inputs(&self) -> &[Input] {
        &self.transaction().inputs
    }
    fn outputs(&self) -> &[Output<Address>] {
        &self.transaction().outputs
    }
    fn total_input(&self) -> Result<Value, InfoError> {
        Ok(self.transaction().total_input()?)
    }
    fn total_output(&self) -> Result<Value, InfoError> {
        Ok(self.transaction().total_output()?)
    }
    fn witnesses(&self) -> &[Witness] {
        match self {
            OneOrTheOther::Tx(_) => &[],
            OneOrTheOther::TxAux(txaux) => &txaux.witnesses,
        }
    }
}
