<?php
/** @param non-empty-array $_bar */
function foo(array $_bar) : void { }

/** @var array{0:list<string>, 1:list<int>} */
$bar = [[], []];

foo($bar);
