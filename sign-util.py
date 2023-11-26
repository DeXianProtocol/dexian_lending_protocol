import sys
import os
from cryptography.hazmat.primitives.asymmetric import ed25519


def sign_message(msg: str, priv_hex: str):
    priv_key = ed25519.Ed25519PrivateKey.from_private_bytes(bytes.fromhex(priv_hex))
    data = bytes(msg.encode("utf-8"))
    signature = priv_key.sign(data).hex()
    # print("{} = {}, {}".format(msg, signature, priv_key.public_key().verify(bytes.fromhex(signature), data)))
    return signature

def print_price_signature(base, quote, price, epoch, timestamp):
    message = "{}/{}{}{}{}".format(base, quote, price, epoch, timestamp)
    priv_key_hex = os.environ.get("DEXIAN_PRICE_ORACLE_PRIV")
    return sign_message(message, priv_key_hex)


if __name__ == '__main__':
    print(print_price_signature(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]))

