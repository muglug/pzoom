<?php
class SomeParent {}
class TypeA extends SomeParent {}
class TypeB extends SomeParent {}

/** @template T of SomeParent */
class Container{
    /** @var T */
    public $value;
    /** @param T $value */
    public function __construct(SomeParent $value) {
        $this->value = $value;
    }
}

/**
 * @template T of SomeParent
 * @param Container<T> $container
 */
function run(Container $container): void{}

run(new Container(new TypeA()));