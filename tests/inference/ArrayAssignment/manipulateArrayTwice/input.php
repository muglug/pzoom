<?php
/** @var array */
$options = [];
$options['a'] = 1;
/** @psalm-suppress MixedArrayAssignment */
$options['b']['c'] = 2;
