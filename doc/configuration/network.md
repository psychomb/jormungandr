
There's 2 differents network interfaces which are covered by their respective section:

```yaml
rest:
   ...
p2p:
   ...
```

## REST interface configuration

- *listen*: listen address
- *pkcs12*: certificate file (optional)
- *cors*: (optional) CORS configuration, if not provided, CORS is disabled
  - *allowed_origins*: (optional) allowed origins, if none provided, echos request origin
  - *max_age_secs*: (optional) maximum CORS caching time in seconds, if none provided, caching is disabled

## P2P configuration

- *trusted_peers*: (optional) the list of nodes to connect to in order to
    bootstrap the p2p topology (and bootstrap our local blockchain);
- *public_id*: (optional) the public identifier send to the other nodes in the
    p2p network. If not set it will be randomly generated.
- *public_address*: the address to listen from and accept connection
    from. This is the public address that will be distributed to other peers
    of the network that may find interest into participating to the blockchain
    dissemination with the node;
- *topics_of_interest*: the different topics we are interested to hear about:
    - *messages*: notify other peers this node is interested about Transactions
    typical setting for a non mining node: `"low"`. For a stakepool: `"high"`;
    - *blocks*: notify other peers this node is interested about new Blocs.
    typical settings for a non mining node: `"normal"`. For a stakepool: `"high"`;
