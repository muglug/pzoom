<?php
namespace Ns;

/** @psalm-param "foo\"with"|"bar"|1|2|3|4.0|4.1 $s */
function foo($s) : void {}
foo("foo\"with");
foo("bar");
foo(1);
foo(2);
foo(3);
foo(4.0);
foo(4.1);
