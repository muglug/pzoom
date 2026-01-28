<?php
/** @template T of string */
class Foo {}
/**
* @template T of non-empty-string
* @extends Foo<T>
*/
class Bar extends Foo {}

/** @param class-string<Foo> $_fooClass */
function bar(string $_fooClass): void {}

bar(Bar::class);
                
