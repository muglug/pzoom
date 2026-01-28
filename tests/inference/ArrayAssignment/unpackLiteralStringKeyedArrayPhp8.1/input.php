<?php
/** @var array<"foo"|"bar", int> */
$x = [];
/** @var array<"baz", int> */
$y = [];

$x = [...$x, ...$y];
                
