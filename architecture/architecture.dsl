workspace "Trading System" "A modular, exchange-agnostic trading system built in Rust" {

    model {
        # People
        trader = person "Trader" "Uses the system to monitor and manage trading operations"
        
        # Software Systems
        tradingSystem = softwareSystem "Trading System" "Core trading system with level-based and strategy-based trading" {
            tags "Internal"
            
            # Exchange Engine
            exchangeEngine = container "Exchange Engine" "Manages exchange state and account data" "Rust" {
                tags "StateManager"
                exchangeStateManager = component "ExchangeStateManager" "Orchestrates state updates, reconciliation, and publishing" "Rust"
                exchangeState = component "ExchangeState" "In-memory state structure" "Rust"
                exchangeStatePublisher = component "ExchangeStatePublisher" "Publishes updates via broadcast channels" "Rust"
                stateReconciler = component "StateReconciler" "Performs WS vs REST reconciliation" "Rust"
                exchangeDriver = component "ExchangeDriver" "Exchange-specific driver (Binance)" "Rust"
            }
            
            # Market Data Engine
            marketDataEngine = container "Market Data Engine" "Manages public market data (orderbooks, trades)" "Rust" {
                tags "StateManager"
                marketDataStateManager = component "MarketDataStateManager" "Orchestrates state updates and publishing" "Rust"
                marketDataState = component "MarketDataState" "In-memory state structure" "Rust"
                marketDataStatePublisher = component "MarketDataStatePublisher" "Publishes updates via broadcast channels" "Rust"
                marketDataDriver = component "MarketDataDriver" "WebSocket driver for market data" "Rust"
            }
            
            # Trading Engine
            tradingEngine = container "Trading Engine" "Core trading logic and execution" "Rust" {
                tags "StateManager"
                tradingStateManager = component "TradingStateManager" "Orchestrates state updates, subscription, and reconciliation" "Rust"
                tradingState = component "TradingState" "In-memory trading state" "Rust"
                exchangeStateSubscriber = component "ExchangeStateSubscriber" "Subscribes to ExchangeStateManager updates" "Rust"
                positionReconciler = component "PositionReconciler" "Reconciles TradingState with ExchangeStateManager" "Rust"
                
                # Level-based trading
                levelManager = component "LevelManager" "Level lifecycle management" "Rust"
                levelExecutor = component "LevelExecutor" "Level execution with indicator resolution" "Rust"
                levelEventLoop = component "LevelEventLoop" "Main event loop for level execution" "Rust"
                indicatorResolver = component "IndicatorResolver" "Resolves price/quantity using indicators" "Rust"
                
                # Strategy-based trading
                strategyExecutor = component "StrategyExecutor" "Strategy execution loop" "Rust"
                strategyManager = component "StrategyManager" "Multi-strategy management" "Rust"
                buyLowSellHigh = component "BuyLowSellHigh" "BuyLowSellHigh strategy implementation" "Rust"
                
                # Risk management
                riskManager = component "RiskManager" "Orchestrates all risk checks" "Rust"
                riskLimits = component "RiskLimits" "Risk limit definitions" "Rust"
                riskMetrics = component "RiskMetrics" "Risk metrics calculation" "Rust"
                circuitBreaker = component "CircuitBreaker" "Circuit breaker pattern implementation" "Rust"
            }
            
            # Indicator Engine
            indicatorEngine = container "Indicator Engine" "Calculates and provides technical indicator values" "Rust" {
                tags "Cache"
                indicatorService = component "IndicatorService" "Orchestrates indicator engine" "Rust"
                indicatorCalculator = component "IndicatorCalculator" "Performs indicator calculations" "Rust"
                marketDataSubscriber = component "MarketDataSubscriber" "Subscribes to market data updates" "Rust"
                indicatorPublisher = component "IndicatorPublisher" "Publishes indicator values" "Rust"
            }
            
            # Asset Engine
            assetEngine = container "Asset Engine" "Caches static asset metadata" "Rust" {
                tags "Cache"
                assetService = component "AssetService" "Orchestrates asset engine" "Rust"
                assetDataCache = component "AssetDataCache" "TTL-based cache for asset data" "Rust"
                binancePublicClient = component "BinancePublicClient" "REST client for public data" "Rust"
            }
            
            # Live Config Service
            liveConfigService = container "Live Config Service" "Runtime configuration updates" "Rust" {
                liveConfigService = component "LiveConfigService" "Orchestrates live config service" "Rust"
                validator = component "Validator" "Validates configuration changes" "Rust"
                levelUpdater = component "LevelUpdater" "Updates level configurations" "Rust"
                riskUpdater = component "RiskUpdater" "Updates risk limits" "Rust"
                auditLogger = component "AuditLogger" "Logs audit entries" "Rust"
            }
            
            # Runner
            runner = container "Runner" "Application orchestrator and gRPC server" "Rust" {
                tags "Orchestrator"
                main = component "Main" "Application entry point" "Rust"
                grpcServer = component "gRPC Server" "gRPC server for external communication" "Rust"
                grpcService = component "TradingCliService" "gRPC service implementation" "Rust"
                shutdownManager = component "ShutdownManager" "Graceful shutdown coordination" "Rust"
            }
        }
        
        # External Systems
        binanceExchange = softwareSystem "Binance Exchange" "Cryptocurrency exchange (spot trading)" {
            tags "External"
            binanceREST = container "Binance REST API" "REST API for account and market data" "HTTPS"
            binanceWebSocket = container "Binance WebSocket" "WebSocket streams for real-time data" "WSS"
        }
        
        mongodb = softwareSystem "MongoDB" "Configuration persistence database" {
            tags "External"
            mongodbInstance = container "MongoDB Instance" "MongoDB database instance" "MongoDB"
        }
        
        tuiCLI = softwareSystem "TUI CLI" "Terminal user interface for monitoring and level management" {
            tags "External"
            tuiApplication = container "TUI Application" "Terminal UI application" "Rust"
            grpcClient = container "gRPC Client" "gRPC client for communication" "Rust"
        }
        
        # Relationships - Trader to TUI CLI
        trader -> tuiCLI "Uses" "HTTPS"
        
        # Relationships - TUI CLI to Trading System
        tuiCLI -> runner.grpcServer "Connects to" "gRPC"
        tuiCLI -> runner.grpcService "Sends requests to" "gRPC"
        
        # Relationships - Trading System to Binance
        exchangeEngine.exchangeDriver -> binanceExchange.binanceWebSocket "Connects to" "WSS"
        exchangeEngine.exchangeDriver -> binanceExchange.binanceREST "Calls" "HTTPS"
        marketDataEngine.marketDataDriver -> binanceExchange.binanceWebSocket "Connects to" "WSS"
        assetEngine.binancePublicClient -> binanceExchange.binanceREST "Calls" "HTTPS"
        tradingEngine.levelExecutor -> binanceExchange.binanceREST "Places orders via" "HTTPS"
        tradingEngine.strategyExecutor -> binanceExchange.binanceREST "Places orders via" "HTTPS"
        
        # Relationships - Trading System to MongoDB
        runner -> mongodb.mongodbInstance "Persists configuration to" "MongoDB Protocol"
        tradingEngine.levelManager -> mongodb.mongodbInstance "Persists levels to" "MongoDB Protocol"
        indicatorEngine.indicatorService -> mongodb.mongodbInstance "Persists indicators to" "MongoDB Protocol"
        liveConfigService -> mongodb.mongodbInstance "Persists config updates to" "MongoDB Protocol"
        
        # Internal Relationships - Channels (ultra-low latency)
        exchangeEngine.exchangeStatePublisher -> tradingEngine.exchangeStateSubscriber "Publishes updates to" "Broadcast Channel"
        exchangeEngine.exchangeStatePublisher -> tradingEngine.riskManager "Publishes updates to" "Broadcast Channel"
        marketDataEngine.marketDataStatePublisher -> indicatorEngine.marketDataSubscriber "Publishes updates to" "Broadcast Channel"
        marketDataEngine.marketDataStatePublisher -> tradingEngine.levelEventLoop "Publishes updates to" "Broadcast Channel"
        indicatorEngine.indicatorPublisher -> tradingEngine.indicatorResolver "Publishes updates to" "Broadcast Channel"
        
        # Internal Relationships - Direct calls
        tradingEngine.levelExecutor -> assetEngine.assetService "Gets trading rules from" "Direct Call"
        tradingEngine.strategyExecutor -> assetEngine.assetService "Gets trading rules from" "Direct Call"
        tradingEngine.levelExecutor -> tradingEngine.riskManager "Checks risk with" "Direct Call"
        tradingEngine.strategyExecutor -> tradingEngine.riskManager "Checks risk with" "Direct Call"
        tradingEngine.levelExecutor -> tradingEngine.tradingStateManager "Updates state via" "Direct Call"
        tradingEngine.strategyExecutor -> tradingEngine.tradingStateManager "Updates state via" "Direct Call"
        
        # Runner orchestrates services
        runner.main -> exchangeEngine "Initializes" "Direct Call"
        runner.main -> marketDataEngine "Initializes" "Direct Call"
        runner.main -> tradingEngine "Initializes" "Direct Call"
        runner.main -> indicatorEngine "Initializes" "Direct Call"
        runner.main -> assetEngine "Initializes" "Direct Call"
        runner.main -> liveConfigService "Initializes" "Direct Call"
        runner.grpcService -> tradingEngine.levelManager "Accesses" "Direct Call"
        runner.grpcService -> exchangeEngine.exchangeStateManager "Accesses" "Direct Call"
        runner.grpcService -> indicatorEngine.indicatorService "Accesses" "Direct Call"
        runner.grpcService -> tradingEngine.tradingStateManager "Accesses" "Direct Call"
        
        # Live Config Service relationships
        liveConfigService -> tradingEngine.levelManager "Updates" "Direct Call"
        liveConfigService -> tradingEngine.riskManager "Updates" "Direct Call"
    }

    views {
        systemContext tradingSystem "SystemContext" {
            include *
            autolayout lr
        }
        
        container tradingSystem "Containers" {
            include *
            autolayout tb
        }
        
        component exchangeEngine "ExchangeEngineComponents" {
            include *
            autolayout tb
        }
        
        component marketDataEngine "MarketDataEngineComponents" {
            include *
            autolayout tb
        }
        
        component tradingEngine "TradingEngineComponents" {
            include *
            autolayout tb
        }
        
        component indicatorEngine "IndicatorEngineComponents" {
            include *
            autolayout tb
        }
        
        component assetEngine "AssetEngineComponents" {
            include *
            autolayout tb
        }
        
        component liveConfigService "LiveConfigServiceComponents" {
            include *
            autolayout tb
        }
        
        component runner "RunnerComponents" {
            include *
            autolayout tb
        }
        
        # Deployment view
        deployment "Production" "Production Deployment" {
            deploymentNode "Trading Server" "Physical server running the trading system" {
                containerInstance tradingSystem.runner
                containerInstance tradingSystem.exchangeEngine
                containerInstance tradingSystem.marketDataEngine
                containerInstance tradingSystem.tradingEngine
                containerInstance tradingSystem.indicatorEngine
                containerInstance tradingSystem.assetEngine
                containerInstance tradingSystem.liveConfigService
            }
            
            deploymentNode "MongoDB Server" "MongoDB database server" {
                containerInstance mongodb.mongodbInstance
            }
            
            deploymentNode "Trader Workstation" "Trader's local machine" {
                containerInstance tuiCLI.tuiApplication
            }
            
            deploymentNode "Binance Exchange" "Binance exchange infrastructure" {
                containerInstance binanceExchange.binanceREST
                containerInstance binanceExchange.binanceWebSocket
            }
        }
        
        styles {
            element "Internal" {
                background #1168bd
                color #ffffff
            }
            element "External" {
                background #999999
                color #ffffff
            }
            element "StateManager" {
                background #438dd5
                color #ffffff
            }
            element "Cache" {
                background #85bbf0
                color #000000
            }
            element "Orchestrator" {
                background #08427b
                color #ffffff
            }
            relationship "Broadcast Channel" {
                color #85bbf0
                dashed true
            }
            relationship "Direct Call" {
                color #999999
            }
            relationship "gRPC" {
                color #438dd5
            }
            relationship "HTTPS" {
                color #999999
            }
            relationship "WSS" {
                color #999999
            }
            relationship "MongoDB Protocol" {
                color #13aa52
            }
        }
    }
}

