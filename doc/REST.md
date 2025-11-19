# REST API Specifications


## Kaspa Networks

The Kaspa network id must be specified in the URL path. Currently, the following networks ids are supported:

- `mainnet`
- `testnet-10`

The indexer must be running on the same network as the network specified in the URL path. Specifying a different network id will result in an error.

## Response Format

All responses are in JSON format. The response result is wrapped in a `Response` object with the following fields:

- `message` - A human-readable message describing the result of the request.
- `result` (optional) - The result of the request.
- `next` (optional) - The offset of the next page.

The `message` field is always present and contains a human-readable message describing the result of the request. The `"success"` text in the message field indicates that the request was successful.

If an error occurs (e.g. a resource is not found or some other error), the `message` field will contain an error message and the HTTP status code will be set to `400`.

Checking that the `message` field contains `"success"` and the HTTP status code is `200` is a good way to check that the request was successful.

## Pagination

### Offsets

Resource listing endpoints are paginated if the number of records is more than the user specified limit or a maximum default limit of `50` records.

If the number of records exceeds the limit, the next page offset will be returned in the response specified within the `next` field.

To obtain the next page, the user must provide the `next` value in the `offset` parameter of the query string.

The following example aggregates all pages until the `next` is `undefined`.
```
fetch page
next = page.next

while next
    fetch page?offset=next
    next = page.next
```

### Direction

The direction of the record iteration can be specified in the query string using the `direction` parameter. The default direction is `forward`.

The following directions are supported:

- `forward` - Iterate from the first record to the last.
- `backward` (or `back`) - Iterate from the last record to the first.

Specifying a `backward` direction will return records in reverse iteration order.

### Limits

The number of records to return can be specified in the query string using the `limit` parameter. The maximum (and the default) limit is `50`. Specifying a limit greater than `50` will be ignored returning maximum `50` records.

## REST endpoints

### Indexer Status

```
GET /api/v1/krc721/{network}/status
```

Response:
```json
{
    "message": "text",
    "result": {
        "version": "string",
        "network": "string",
        "isNodeConnected": true,
        "isNodeSynced": true,
        "isIndexerSynced": true,
        "lastKnownBlockHash": "string",
        "daaScore": "uint64",
        "powFeesTotal": "uint64",
        "royaltyFeesTotal": "uint64",
        "tokenDeploymentsTotal": "uint64",
        "tokenMintsTotal": "uint64",
        "tokenTransfersTotal": "uint64"
    }
}
```

### Collections

#### Get Collections List
```
GET /api/v1/krc721/{network}/nfts
```

Response:
```json
{
    "message": "text",
    "prev": "text",
    "next": "text",
    "result": [
        {
            "deployer": "kaspatest:qqqqqqqqqqqqqqq",
            "buri": "ipfs://QOWmd",
            "max": "250",
            "daaMintStart": "0",
            "premint": "6",
            "tick": "FOO",
            "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
            "mtsAdd": "1000000000",
            "minted": "6",
            "opScoreMod": "1000000000",
            "state": "deployed",
            "mtsMod": "1000000000",
            "opScoreAdd": "1000000000"
        }
    ],
    "next": 13000000000000
}
```

#### Get Collection Details
```
GET /api/v1/krc721/{network}/nfts/{tick}
```

Response:
```json
{
    "message": "text",
    "result":
    {
        "deployer": "kaspatest:qqqqqqqqqqqqqqqqq",
        "royaltyTo": "kaspatest:qqqqqqqqqqqqqqqqqqq",
        "buri": "ipfs://dz1...",
        "max": "800",
        "royaltyFee": "2500000000",
        "daaMintStart": "0",
        "premint": "10",
        "tick": "FOO",
        "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
        "mtsAdd": "1000000000",
        "minted": "556",
        "opScoreMod": "1000000000",
        "state": "deployed",
        "mtsMod": "1000000000",
        "opScoreAdd": "1000000000"
    }
}
```

### Tokens

#### Get Token Details
```
GET /api/v1/krc721/{network}/nfts/{tick}/{id}
```

Response:
```json
{
    "message": "text",
    "result":
    {
        "tick": "FOO",
        "tokenId": "123",
        "owner": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        "buri": "ipfs://..."
    }
}
```

#### Get Token Owners
```
GET /api/v1/krc721/{network}/owners/{tick}
```

Response:
```json
{
    "message": "text",
    "result": [
        {
            "tick": "FOO",
            "tokenId": "123",
            "owner": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            "opScoreMod": "1000000000"
        }
    ],
    "next": 51
}
```

### Address Holdings

#### Get Address NFT List
```
GET /api/v1/krc721/{network}/address/{address}
```

Response:
```json
{
    "message": "text",
    "result": [
        {
            "tick": "FOO",
            "tokenId": "381",
            "buri": "ipfs://..."
        },
        {
            "tick": "FOO",
            "tokenId": "382",
            "buri": "ipfs://..."
        },
        {
            "tick": "FOO",
            "tokenId": "31010",
            "buri": "ipfs://..."
        }
    ],
    "next":"FOO-123"
}

```


#### Get Address Collection Holdings
```
GET /api/v1/krc721/{network}/address/{address}/{tick}
```

Response:
```json
{
    "message": "text",
    "result":
    {
        "tick": "FOO",
        "tokenId": "381",
        "opScoreMod": "79993666"
    }
}

```


### Operations

#### Get Operations List
```
GET /api/v1/krc721/{network}/ops
```

Response:
```json
{
    "message": "text",
    "result": [
        {
            "p": "krc-721",
            "deployer": "kaspatest:kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            "op": "deploy",
            "tick": "FOO",
            "opData": {
                "buri": "ipfs://...",
                "max": "456"
            },
            "opScore": "123",
            "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
            "mtsAdd": "00000000000000", 
            "opError": "InsufficientFee",
            "feeRev": "123"
        },
        {
            "p": "krc-721",
            "deployer": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            "op": "mint",
            "tick": "FOO",
            "opData": {
                "tokenId": "1234",
                "to": "0000"
            },
            "opScore": "123",
            "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
            "opError": "InsufficientFee",
            "mtsAdd": "00000000000000",
            "feeRev": "123"
        },    
        {
            "p": "krc-721",
            "deployer": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
            "op": "transfer",
            "tick": "FOO",
            "opData": {
                "tokenId": "1234",
                "to": "0000"
            },
            "opScore": "123",      
            "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
            "opError": "InsufficientFee",
            "mtsAdd": "00000000000000",
            "feeRev": "123"
        }
    ],
    "next": 13000000000000
}

```

#### Get Operation Details by Score
```
GET /api/v1/krc721/{network}/ops/score/{id}
```

Response:
```json
{
    "message": "text",
    "result":
    {
        "p": "krc-721",
        "deployer": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        "royalty_to": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        "tick": "FOO",
        "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
        "mtsAdd": "00000000000000",
        "op": "deploy",
        "opData": {
            "buri": "ipfs://",
            "max": "10000",
            "royaltyFee": "12300000",
            "daaMintStart": "0",
            "premint": "123"
        },
        "opError": "InsufficientFee",
        "opScore": "123",
        "feeRev": "123"
    }
}

```

#### Get Operation Details by Transaction ID
```
GET /api/v1/krc721/{network}/ops/txid/{txid}
```

Response:
```json
{
    "message": "text",
    "result":
    {
        "p": "krc-721",
        "deployer": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        "to": "kaspatest:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
        "tick": "FOO",
        "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000",
        "mtsAdd": "0000000000000",
        "op": "deploy",
        "opData": {
            "buri": "ipfs://",
            "max": "1230",
            "daaMintStart": "0"
        },
        "opError": "InsufficientFee",
        "opScore": "123",
        "feeRev": "123"
    }
}

```

#### Get Royalty Fees for a given address and tick
```
GET /api/v1/krc721/{network}/royalties/{address}/{tick}
```

Response:
```json
{
    "message": "text",
    "result": "1000000000"
}
```

#### Get Rejection Reason by Transaction ID

Rejections include the reason for which the indexer has rejected the transaction. Rejections can occur due to an invalid operation (insufficient fee, invalid ticker, already deployed ticker, etc.) or due to a static check failure (e.g. missing `"to"` field in the transfer operation).

A transaction is recorded in the indexer log only if it contains a valid krc721 envelope (all other transactions are ignored).

```
GET /api/v1/krc721/{network}/rejections/txid/{txid}
```

Response:
```json
{
    "message": "text",
    "result": "rejection reason"
}
```

### Get Ownership History
```
GET /api/v1/krc721/{network}/history/{tick}/{id}
```

Get ownership history of a token.

Response:
```json
{
  "message": "success",
  "result": [
    {
      "owner": "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
      "opScoreMod": "2000000000",
      "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    {
      "owner": "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
      "opScoreMod": "1002200000",
      "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    {
      "owner": "kaspa:qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
      "opScoreMod": "1000000001",
      "txIdRev": "0000000000000000000000000000000000000000000000000000000000000000"
    }
  ],
  "next": "100000000"
}
```

### Get Available Token ID Ranges
```
GET /api/v1/krc721/{network}/ranges/{tick}
```
Get available token ID ranges for minting in a collection.
Response:

```json 
{
    "message": "success",
    "result": "100,50,200,20,5000,99990"
}
```
The result string represents available ranges in format start1,size1,start2,size2,.... For each range, start is the starting token ID and size is how many consecutive token IDs are available starting from that ID.

Response when fully minted:
```json
{
    "message": "success",
    "result": ""
}
```
