<?php
/**
 * @method static int bar()
 */
class A {}
class B extends A {}
function consumeInt(int $i): void {}

/** @psalm-suppress UndefinedMethod, MixedArgument */
consumeInt(B::bar());
