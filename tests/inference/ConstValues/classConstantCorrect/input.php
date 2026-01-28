<?php
namespace Ns;

class C {
    const A1 = "bat";
    const B = "baz";
}
/** @psalm-param "foo"|"bar"|C::A1|C::B $s */
function foo($s) : void {}
foo("foo");
foo("bar");
foo("bat");
foo("baz");
