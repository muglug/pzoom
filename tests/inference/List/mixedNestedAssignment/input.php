<?php
/** @psalm-suppress MissingReturnType */
function getMixed() {}

/**
 */
list($a, list($b, $c)) = getMixed();
