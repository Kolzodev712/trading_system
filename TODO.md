- create a general redis integration to dump all WS data into an ephemeral storage which other clients can subscribe to for real-time updates - WIP

- create the indicator/computation module logic where we get raw data from an exchange and compute it into a trigger value - WIP

- create trades parsing logic that derives relevant data - WIP

- Refactor the exchange engine to accept multiple integrations into the driver (simiar to market data engine logic)




- create a basic indicator from the Binance orderbook or trades data - Not started

- create a basic strategy and run it in the runner.rs module - Not started

- run the entire application on a docker container