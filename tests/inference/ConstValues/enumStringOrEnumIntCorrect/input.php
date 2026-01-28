<?php
namespace Ns;

/** @psalm-param ( "foo\"with" | "bar" | 1 | 2 | 3 ) $s */
function foo($s) : void {}
foo("foo\"with");
foo("bar");
foo(1);
foo(2);
foo(3);
