<?php
class Serializer { public function unserialize(string $_s): mixed { return null; } }

/** @template T */
class Cache {
    /** @var array<string, list{string, T}> */
    private array $cache = [];

    public function __construct(private Serializer $serializer) {}

    public function load(string $contents): void {
        /** @var array<string, list{string, T}> */
        $this->cache = $this->serializer->unserialize($contents);
        if ($this->cache === []) {
            echo 'empty';
        }
    }
}
