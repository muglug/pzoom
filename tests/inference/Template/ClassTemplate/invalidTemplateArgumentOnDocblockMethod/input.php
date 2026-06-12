<?php
/**
 * @template T
 * @method void set(T $value)
 */
class Container
{
    public function __call(string $name, array $args) {}
}

class A {}
class B {}

/** @var Container<A> $container */
$container = new Container();
$container->set(new B());
