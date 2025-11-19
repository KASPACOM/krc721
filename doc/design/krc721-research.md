[KRC-721](krc721.md)
-> [KRC-721 Research](krc721-research.md)

# NFT research

Everything to know to get started on KRC-721.


# ERC-721

https://docs.openzeppelin.com/contracts/3.x/api/token/erc721#ERC721

## EIP-721

https://eips.ethereum.org/EIPS/eip-721

## ERC-1155

https://docs.openzeppelin.com/contracts/3.x/erc1155

```json
{
    "name": "Thor's hammer",
    "description": "Mjölnir, the legendary hammer of the Norse god of thunder.",
    "image": "https://game.example/item-id-8u5h2m.png",
    "strength": 20
}
```

# Digital art and rarity

Digital art can be achieved by using a hash as RNG seed to set the attributes.

Large attributes sets require and external JSON metadata file hostable on IPFS.

Rarity could be set like in the ordinals.

## Question 0: how is the combination of attributes when mint occurs generated ?

### How Ordinals do it
**Rarity**: rarity determined by its position in the sequence, block height influencing its rarity level: common, uncommon, rare, epic, legendary, mythic.

**Exotics**: Exotics are ordinals with unique properties: integer square or cube roots, connections to historical events.

## Question 1: to allow approvability of sending NFT or not ?

For example: like for solana saga 2 smartphone sending immutably owned NFTs to those who bought it 

--> Should we add that spec to KRC-721 ?

Anton: *Always spendable*

## Question 2: Metadata via external uri possible ?

Metadata contains name, description and image. Also a list attributes.

Anton: *inscribed*


# BRC-721

NFTs on Bitcoin via inscriptions 
https://github.com/adshao/brc-721?tab=readme-ov-file

# Ordinals

https://docs.ordinals.com/overview.html


How long metadata is handled:

```t
OP_FALSE
OP_IF
    ...
    OP_PUSH 0x05 OP_PUSH '{"very":"long","metadata":'
    OP_PUSH 0x05 OP_PUSH '"is","finally":"done"}'
    ...
OP_ENDIF
```

**Note**: the ID is called "value", has preview and content link. Exotic teleburn feature to burn assets on other blockchains


## External metadata

**Note** the external link resolves to the metadata of the NFT

```json
    {
        "p": "brc-721",
        "op": "deploy",
        "tick": "ordinals",
        "max": "10000",
        "buri": "https://ipfs.io/abc/"
    }
```

## Immutable

**Note** the metadata is given at the deployment phase

```json
    {
        "p": "brc-721",
        "op": "deploy",
        "tick": "ordinals",
        "max": "10000",
        "meta": {
            "name": "Ordinals",
            "description": "Bring NFT to Kaspa", 
            "image": "https://storage.googleapis.com/opensea-prod.appspot.com/puffs/3.png",
            "attributes": [
                {
                    "trait_type": "trait1", 
                    "value": "value1"
                }, ... ]
        },
    }
```

## Typescript interface

https://github.com/bitcoin-computer/BRC721/blob/main/src/brc721.ts

```typescript
    interface IBRC721 {
        mint(to: string, name?: string, symbol?: string): Promise<NFT>
        balanceOf(publicKey: string): Promise<number>
        ownerOf(tokenId: string): Promise<string[]>
        transfer(to: string, tokenId: string)
    }
```
