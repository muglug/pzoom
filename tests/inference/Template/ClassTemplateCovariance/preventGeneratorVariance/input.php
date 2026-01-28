<?php
class Foo {
    function a(): void {}
}

class Bar extends Foo {
  function b(): void {}
}

/**
 * @return Generator<int, Bar, Bar, mixed>
 */
function gen() : Generator {
  $bar = yield new Bar();
  $bar->b();
}

/** @param Generator<int,Bar,Foo,mixed> $gen */
function sendFoo(Generator $gen): void {
  $gen->send(new Foo());
}

$gen = gen();
sendFoo($gen);
