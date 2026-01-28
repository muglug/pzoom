<?php
/** @var array{foo: mixed, bar: mixed} $arr */
$a = array_pad($arr, 5, null);
/** @var mixed $mixed */
$b = array_pad([$mixed, $mixed], 5, null);
/** @var list $list */
$c = array_pad($list, 5, null);
/** @var mixed[] $array */
$d = array_pad($array, 5, null);
