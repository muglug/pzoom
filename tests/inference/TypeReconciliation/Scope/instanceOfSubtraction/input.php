<?php
class Foo {}
class FooBar extends Foo{}
class FooBarBat extends FooBar{}
class FooMoo extends Foo{}

$a = new Foo();

if ($a instanceof FooBar && !$a instanceof FooBarBat) {

} elseif ($a instanceof FooMoo) {

}