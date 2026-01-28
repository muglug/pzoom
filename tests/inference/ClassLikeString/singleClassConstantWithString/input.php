<?php
/**
 * @param class-string $s
 */
function takesClassConstants(string $s) : void {}

class A {}

/** @psalm-suppress ArgumentTypeCoercion */
takesClassConstants("A");
