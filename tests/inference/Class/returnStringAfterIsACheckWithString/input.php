<?php
class Foo{}
/** @param class-string $maybeBaz */
function bar(string $maybeBaz) : string {
  if (!is_a($maybeBaz, Foo::class, true)) {
    throw new Exception("not Foo");
  }
  return $maybeBaz;
}
