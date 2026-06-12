<?php
function matches(string $value): bool {
    if ("\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1" !== $value) {
        return false;
    }
    return true;
}
