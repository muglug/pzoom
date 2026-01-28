<?php
trait InstancePool {
    /**
     * @template T as self
     * @param callable():?T $callback
     * @return ?T
     */
    public static function getInstance(callable $callback)
    {
        return $callback();
    }
}

class Foo
{
    use InstancePool;
}

class Bar
{
    public function a(): void
    {
        Foo::getInstance(function () {
            return new Foo();
        });
    }
}