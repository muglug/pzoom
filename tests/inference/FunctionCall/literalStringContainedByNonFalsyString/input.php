<?php
/** @param non-falsy-string $s */
function takesNonFalsy(string $s): void {}

takesNonFalsy('$GLOBALS');
takesNonFalsy('0');
takesNonFalsy('');

class VF {
    public const SUPER_GLOBALS = ['$GLOBALS', '$_SERVER', '$_GET'];
}
function f(string $name): bool {
    return in_array('$' . $name, VF::SUPER_GLOBALS, true);
}
