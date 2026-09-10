local os = std.extVar('OS');
local cacheDirectory = std.extVar('XDG_CACHE_HOME') + '/bb_clientd';

{
  clusters:: {
    'cyclops.buildbuddy.io': 'cyclops.buildbuddy.io',
  },

  casBlocksSizeBytes:: 20 * 1024 * 1024 * 1024,
  filePoolSizeBytes:: 20 * 1024 * 1024 * 1024,
  averageCasBlobSizeBytes:: 5 * 1024,
  casKeyLocationMapSizeBytes:: std.ceil((self.casBlocksSizeBytes * 66) / (self.averageCasBlobSizeBytes * 0.5)),

  acBlocksSizeBytes:: 1024 * 1024 * 1024,
  averageAcBlobSizeBytes:: 1024,
  acKeyLocationMapSizeBytes:: std.ceil((self.acBlocksSizeBytes * 66) / (self.averageAcBlobSizeBytes * 0.5)),

  maximumMessageSizeBytes: 16 * 1024 * 1024,
  maximumTreeSizeBytes: 256 * 1024 * 1024,
  authorizationHeader:: null,
  proxyURL:: '',
  useNFSv4:: os == 'Darwin',

  grpcClient:: function(hostname, authorizationHeader, proxyURL) {
    address: hostname + ':443',
    tls: {},
    [if authorizationHeader != null then 'addMetadata']: [
      {
        header: 'authorization',
        values: [authorizationHeader],
      },
    ],
    addMetadataJmespathExpression: {
      expression: |||
        {
          "build.bazel.remote.execution.v2.requestmetadata-bin": incomingGRPCMetadata."build.bazel.remote.execution.v2.requestmetadata-bin"
        }
      |||,
    },
    keepalive: {
      time: '60s',
      timeout: '30s',
    },
    proxyUrl: proxyURL,
  },

  blobstoreConfig:: function(authorizationHeader, proxyURL) {
    demultiplexing: {
      instanceNamePrefixes: {
        [cluster]: {
          backend: {
            grpc: {
              client: $.grpcClient($.clusters[cluster], authorizationHeader, proxyURL),
            },
          },
        }
        for cluster in std.objectFields($.clusters)
      },
    },
  },

  blobstore: {
    actionCache: {
      demultiplexing: {
        instanceNamePrefixes: {
          local: {
            backend: {
              local: {
                keyLocationMapOnBlockDevice: {
                  file: {
                    path: cacheDirectory + '/ac/key_location_map',
                    sizeBytes: $.acKeyLocationMapSizeBytes,
                  },
                },
                keyLocationMapMaximumGetAttempts: 16,
                keyLocationMapMaximumPutAttempts: 64,
                oldBlocks: 1,
                currentBlocks: 5,
                newBlocks: 1,
                blocksOnBlockDevice: {
                  source: {
                    file: {
                      path: cacheDirectory + '/ac/blocks',
                      sizeBytes: $.acBlocksSizeBytes,
                    },
                  },
                  spareBlocks: 1,
                },
                persistent: {
                  stateDirectoryPath: cacheDirectory + '/ac/persistent_state',
                  minimumEpochInterval: '300s',
                },
              },
            },
          },
          '': {
            backend: $.blobstoreConfig($.authorizationHeader, $.proxyURL),
          },
        },
      },
    },
    contentAddressableStorage: {
      withLabels: {
        backend: {
          demultiplexing: {
            instanceNamePrefixes: {
              local: {
                backend: {
                  label: 'localCAS',
                },
              },
              '': {
                backend: {
                  readCaching: {
                    slow: {
                      existenceCaching: {
                        backend: {
                          label: 'clustersCAS',
                        },
                        existenceCache: {
                          cacheSize: 1000 * 1000,
                          cacheDuration: '300s',
                          cacheReplacementPolicy: 'LEAST_RECENTLY_USED',
                        },
                      },
                    },
                    fast: {
                      label: 'localCAS',
                    },
                    replicator: {
                      deduplicating: {
                        concurrencyLimiting: {
                          base: {
                            local: {},
                          },
                          maximumConcurrency: 100,
                        },
                      },
                    },
                  },
                },
              },
            },
          },
        },
        labels: {
          localCAS: {
            local: {
              keyLocationMapOnBlockDevice: {
                file: {
                  path: cacheDirectory + '/cas/key_location_map',
                  sizeBytes: $.casKeyLocationMapSizeBytes,
                },
              },
              keyLocationMapMaximumGetAttempts: 16,
              keyLocationMapMaximumPutAttempts: 64,
              oldBlocks: 1,
              currentBlocks: 5,
              newBlocks: 1,
              blocksOnBlockDevice: {
                source: {
                  file: {
                    path: cacheDirectory + '/cas/blocks',
                    sizeBytes: $.casBlocksSizeBytes,
                  },
                },
                spareBlocks: 1,
                dataIntegrityValidationCache: {
                  cacheSize: 100000,
                  cacheDuration: '14400s',
                  cacheReplacementPolicy: 'LEAST_RECENTLY_USED',
                },
              },
              persistent: {
                stateDirectoryPath: cacheDirectory + '/cas/persistent_state',
                minimumEpochInterval: '300s',
              },
            },
          },
          clustersCAS: $.blobstoreConfig($.authorizationHeader, $.proxyURL),
        },
      },
    },
  },

  schedulers: {
    [cluster]: {
      endpoint: $.grpcClient($.clusters[cluster], $.authorizationHeader, $.proxyURL),
    }
    for cluster in std.objectFields($.clusters)
  },

  grpcServers: [
    {
      listenPaths: [cacheDirectory + '/grpc'],
      authenticationPolicy: {
        allow: {},
      },
    },
  ],

  mount: {
    mountPath: cacheDirectory + '/mount',
  } + if $.useNFSv4 then {
    nfsv4: {
      enforcedLeaseTime: '120s',
      announcedLeaseTime: '60s',
    } + {
      Darwin: {
        darwin: {
          socketPath: cacheDirectory + '/nfsv4',
        },
      },
      Linux: {
        linux: {
          mountOptions: ['vers=4.1'],
        },
      },
    }[os],
  } else {
    fuse: {
      directoryEntryValidity: '300s',
      inodeAttributeValidity: '300s',
      allowOther: false,
    },
  },

  filePool: {
    blockDevice: {
      file: {
        path: cacheDirectory + '/filepool',
        sizeBytes: $.filePoolSizeBytes,
      },
    },
  },
  outputPathPersistency: {
    stateDirectoryPath: cacheDirectory + '/outputs',
    maximumStateFileSizeBytes: 1024 * 1024 * 1024,
    maximumStateFileAge: '604800s',
  },
  directoryCache: {
    maximumCount: 10000,
    maximumSizeBytes: 1024 * self.maximumCount,
    cacheReplacementPolicy: 'LEAST_RECENTLY_USED',
  },
  maximumFileSystemRetryDelay: '300s',
  global: {
    logPaths: [cacheDirectory + '/log'],
    [if $.authorizationHeader == null then 'grpcForwardAndReuseMetadata']: ['authorization'],
  },
}
