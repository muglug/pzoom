<?php
/**
 * @template T
 * @method T get()
 * @method void set(T $value)
 */
class Container
{
    public function __call(string $name, array $args) {}
}

class A {}
function foo(A $a): void {}

/** @var Container<A> $container */
$container = new Container();
$container->set(new A());
foo($container->get());
                