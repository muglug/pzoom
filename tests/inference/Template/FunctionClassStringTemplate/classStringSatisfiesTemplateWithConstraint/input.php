<?php
/** @template T of string */
class Foo {}

/** @param class-string<Foo> $_fooClass */
function bar(string $_fooClass): void {}

bar(Foo::class);
                
