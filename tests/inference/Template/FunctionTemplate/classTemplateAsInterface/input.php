<?php
interface Foo {}
interface FooChild extends Foo {}
class FooImplementer implements Foo {}

/**
 * @template T as Foo
 * @param T $x
 * @return T
 */
function bar($x) {
    return $x;
}

function takesFoo(Foo $f) : void {
    bar($f);
}

function takesFooChild(FooChild $f) : void {
    bar($f);
}

function takesFooImplementer(FooImplementer $f) : void {
    bar($f);
}