<?php

namespace X;

final class A {}

$_a = new A();
/** @psalm-check-type-exact $_a = A */
