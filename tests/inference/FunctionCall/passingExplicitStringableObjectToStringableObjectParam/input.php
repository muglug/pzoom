<?php
/** @param stringable-object $o */
function acceptsStringableObject(object $o): void {}

class C implements Stringable { public function __toString(): string { return __CLASS__; }}

acceptsStringableObject(new C);
