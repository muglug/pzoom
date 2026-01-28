<?php
/**
 * @template T of array<int, mixed>
 */
class Foo
{
}

/**
 * @param Foo<array<int, DateTime>> $a
 */
function bar(Foo $a) : void {}