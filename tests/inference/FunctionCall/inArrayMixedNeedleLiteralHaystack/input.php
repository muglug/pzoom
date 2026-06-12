<?php
/** @param lowercase-string $name */
function isWellKnown(string $name): bool {
    return in_array($name, ['from', 'tryFrom'], true);
}
