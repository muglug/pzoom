<?php
/**
 * @method string foo()
 */
interface I {}
interface I2 extends I {}
function consumeString(string $s): void {}

/** @var I2 $i */
consumeString($i->foo());
