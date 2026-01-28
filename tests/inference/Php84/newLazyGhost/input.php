<?php
class Foo {}
$reflectionClass = new ReflectionClass(Foo::class);
$lazyGhost = $reflectionClass->newLazyGhost(function (Foo $foo) {});
