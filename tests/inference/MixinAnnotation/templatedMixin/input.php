<?php

/**
 * @template T
 */
abstract class Foo {
    /** @return T */
    abstract public function hi();
}

/**
 * @mixin Foo<string>
 */
class Bar {}

$bar = new Bar();
$b = $bar->hi();
