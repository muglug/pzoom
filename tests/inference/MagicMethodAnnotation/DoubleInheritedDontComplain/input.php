<?php
/**
 * @method void func(int ...$args)
 */
class Foo {}

class Bar extends Foo {
    public function func(int $i = 0, int ...$args): void {}
}

class UhOh extends Bar {}
