- create http client for binance for signed and not signed messages - Done
- implement open order, cancel order by id and cancel all orders - Done
- implement market data engine with WS support for binance orderbooks and trades - Done
- implement channel logic for passing the stream to the computation engine - Done
- test both the streaming and exchange trigger logic inside the computation module - Done
- implement the exchange and market data generic logic on the binance driver and test all methods using the common trait logic - DONE

- create the indicator/computation module logic where we get raw data from an exchange and compute it into a trigger value - Planning

- create orderbook parsing logic that derives a mid_price and spread value - DONE

- create trades parsing logic that derives relevant data - WIP

- create a basic indicator from the Binance orderbook or trades data - Not started

- create a basic strategy and run it in the runner.rs module - Not started

- run the entire application on a docker container