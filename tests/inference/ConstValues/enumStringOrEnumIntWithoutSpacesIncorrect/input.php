<?php
namespace Ns;

/** @psalm-param "foo\"with"|"bar"|1|2|3 $s */
function foo($s) : void {}
foo(4);
