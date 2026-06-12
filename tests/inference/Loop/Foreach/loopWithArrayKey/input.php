<?php
/**
 * @param array<array<int, array<string, string>>> $args
 * @return array[]
 */
function get_merged_dict(array $args) {
    $merged = array();

    foreach ($args as $group) {
        foreach ($group as $key => $value) {
            if (isset($merged[$key])) {
                $merged[$key] = array_merge($merged[$key], $value);
            } else {
                $merged[$key] = $value;
            }
        }
    }

    return $merged;
}
