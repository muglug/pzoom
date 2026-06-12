<?php

class Wrapped {}

function constrain(Wrapped &$stmt_type): void
{
    $stmt_type = new Wrapped();
}

function fillNullable(?Wrapped &$value_type): void
{
    $value_type = new Wrapped();
}

/** @param list<int|string> $atomics */
function process(array $atomics): ?Wrapped
{
    $new_assign_type = null;
    $array_access_value_type = null;

    foreach ($atomics as $atomic) {
        if (is_int($atomic)) {
            $new_assign_type = new Wrapped();
            constrain($new_assign_type);
        } elseif (is_string($atomic)) {
            fillNullable($array_access_value_type);
            $new_assign_type = $array_access_value_type;
        }
    }
    return $new_assign_type;
}
