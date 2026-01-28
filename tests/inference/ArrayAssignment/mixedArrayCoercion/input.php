<?php
/** @param int[] $arg */
function expect_int_array($arg): void { }
/** @return array */
function generic_array() { return []; }

/** @psalm-suppress MixedArgumentTypeCoercion */
expect_int_array(generic_array());

function expect_int(int $arg): void {}

/** @return mixed */
function return_mixed() { return 2; }

/** @psalm-suppress MixedArgument */
expect_int(return_mixed());
