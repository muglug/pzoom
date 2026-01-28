<?php

/** @var list<array{a: int}> */
$a = [];

$b = array_merge(...$a);

/** @var non-empty-list<array{a: int}> */
$c = [];

$d = array_merge(...$c);
                
