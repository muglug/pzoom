<?php

final class test {}

/** @var array|int|float|test|null */
$a = null;
if (\is_object($a)) {
}
