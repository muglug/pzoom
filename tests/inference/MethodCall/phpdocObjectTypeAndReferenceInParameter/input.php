<?php
class Foo {
    /**
     * @param object $object
     */
    public function bar(&$object): void {}
}
$x = new Foo();
$x->bar($x);
