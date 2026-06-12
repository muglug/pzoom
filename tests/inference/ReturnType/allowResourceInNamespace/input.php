<?php

namespace Bar;

class Resource {
    function get(string $key): ?string {
        return "";
    }
}

class Foo {
    /** @var string[] */
    private $references = [];

    /** @var Resource */
    private $resource;

    public function __construct() {
        $this->resource = new Resource();
    }

    public function foo(): array {
        $types = [];

        foreach ($this->references as $ref => $data) {
            $types[$ref] = $this->resource->get($data);
        }

        return $types;
    }
}
