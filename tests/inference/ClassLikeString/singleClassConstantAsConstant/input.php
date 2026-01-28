<?php
/**
 * @param class-string $s
 */
function takesClassConstants(string $s) : void {}

class A {}

takesClassConstants(A::class);
