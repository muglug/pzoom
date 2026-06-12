<?php
namespace Foo;

/**
 * @method string bar(\DateTimeInterface|\DateInterval|self $a, Cache|\Exception $e)
 */
class Cache {
    public function __call(string $method, array $args) {
        return $method;
    }
}

(new Cache)->bar(new \DateTime(), new Cache());
