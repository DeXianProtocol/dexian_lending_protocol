## Price Oracle
综观整个DeXian Lending Protocol，我们需要一个可信赖、及时、安全价格喂送机制。然而Radix Babylon上线时间不久，目前还没有大规模和广泛使用的价格预言机，我们已经知道有[两家](https://twitter.com/jimmyhumania/status/1725867423566299605)提可供了Scrypto原生整合接口，但是它们的更新不及时有时是[半小时](https://dashboard.radixdlt.com/component/component_rdx1czqqs4t8f62jeyp47ctyqwmtk3vnf9sffnqd9lu7tgtgtvshj6x9lp/recent-transactions)~[几小时](https://dashboard.radixdlt.com/component/component_rdx1czqqs4t8f62jeyp47ctyqwmtk3vnf9sffnqd9lu7tgtgtvshj6x9lp/recent-transactions)更新一次，这对于协议的参与者来说会带来额外风险。

Throughout the DeXian Lending Protocol, we require reliable, up to date, and secure price feeds. However Radix Babylon has not been online long enough for there to be a large-scale and widely available price prediction machine. We already know of two that offer interfaces that can be integrated natively in Scrypto, but they are not updated in a timely manner, some of them half an hour ~ a few hours, which poses an additional risk for participants in the protocol.

## Price Feed(endpoint)
我们结合实现情况设计了中立角色提供使用密码学签名的价格信息，并通过任何人可以验证的方法校验其数据的有效性。如图：


#### Price Feed Provider
* 目前的价格来源: [coingecko](https://www.coingecko.com/en/coins/radix), [bitfinex](https://trading.bitfinex.com/t/XRD:USD),[gate.io](https://www.gate.io/zh/trade/XRD_USDT), [kucoin](https://www.kucoin.com/trade/XRD-USDT)

* 对上述价格进行使用密码学签名并公开公钥以便任何人都可以签证信息，通过访问[Endpoint](https://dashboard.radixdlt.com/component/component_rdx1czqqs4t8f62jeyp47ctyqwmtk3vnf9sffnqd9lu7tgtgtvshj6x9lp/recent-transactions)可以获得。

* 价格提供是角色中立身份，为了提供安全，及时，可靠的价格信息。

#### Public verifiable

* 价格信息中包含了价格、时间戳、epoch以及token地址信息以防止被篡改或重放。
* 价格信息明文展示，人类可读，密码学双重可验证。

#### DeXian Lending Protocol participants

* 借款，续借，清算等参与者与协议交互时需要引用价格提供者的价格信息。
* DeXian Lending Protocol的Scrypto组件使用密码学方法验证其数据有效后，方可进行下一步交互操作。
* 需要进行协议整合的开发者及项目方通过X,Telegram反馈需求。