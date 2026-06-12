<?php
class Foo {
    public static function barBar(): void {}
}

/** @var mixed */
$a = (new Foo());

$a->barBar();
