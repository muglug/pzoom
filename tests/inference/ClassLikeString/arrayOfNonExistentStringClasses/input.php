<?php
/**
 * @param array<class-string> $arr
 */
function takesClassConstants(array $arr) : void {}
/** @psalm-suppress ArgumentTypeCoercion */
takesClassConstants(["A", "B"]);
