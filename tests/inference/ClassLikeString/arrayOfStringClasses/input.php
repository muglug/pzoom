<?php
/**
 * @param array<class-string> $arr
 */
function takesClassConstants(array $arr) : void {}

class A {}
class B {}

takesClassConstants(["A", "B"]);
