<?php
/** @param array<string, string> $if_types */
function wrapTypes(array $if_types): bool {
    $if_types = $if_types ? [$if_types] : [];

    if ($if_types === []) {
        return false;
    }

    return true;
}
