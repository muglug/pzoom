<?php
class Foo {}
class Bar {}
$reflectionClass = new ReflectionClass(Foo::class);
$reflectionClass->newLazyProxy(fn(Bar $bar) => new Foo);
