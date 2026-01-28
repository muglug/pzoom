<?php
/**
 * @template T
 * @method T get()
 * @method void set(T $value)
 */
interface Container
{
}

class A {}
function foo(A $a): void {}

/** @var Container<A> $container */
$container->set(new A());
foo($container->get());
                