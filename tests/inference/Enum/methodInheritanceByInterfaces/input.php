<?php
interface I extends BackedEnum {}
/** @var I $i */
$a = $i::cases();
$b = $i::from(1);
$c = $i::tryFrom(2);
