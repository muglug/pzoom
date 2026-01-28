<?php
class Foo {}
class FooChild extends Foo {}

/**
 * @template T as Foo
 * @param T $x
 * @return T
 */
function bar($x) {
    return $x;
}

bar(new Foo());
bar(new FooChild());