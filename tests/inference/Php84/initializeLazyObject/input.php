<?php
class Foo {}
$reflectionClass = new ReflectionClass(Foo::class);
$lazyProxy = $reflectionClass->newLazyProxy(fn() => new Foo);
$realInstance = $reflectionClass->initializeLazyObject($lazyProxy);
