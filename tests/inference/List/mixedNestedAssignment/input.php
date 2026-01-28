<?php
/** @psalm-suppress MissingReturnType */
function getMixed() {}

/**
 * @psalm-suppress MixedArrayAccess
 * @psalm-suppress MixedAssignment
 */
list($a, list($b, $c)) = getMixed();
