<?php
function contains(array $data, array $needle): bool {
    if (empty($data) || empty($needle)) {
        return false;
    }
    $stack = [];

    while (!empty($needle)) {
        $key = key($needle);
        $val = $needle[$key];
        unset($needle[$key]);

        if (array_key_exists($key, $data) && is_array($val)) {
            $next = $data[$key];
            unset($data[$key]);

            if (!empty($val)) {
                $stack[] = [$val, $next];
            }
        } elseif (!array_key_exists($key, $data) || $data[$key] != $val) {
            return false;
        }

        if (empty($needle) && !empty($stack)) {
            list($needle, $data) = array_pop($stack);
        }
    }

    return true;
}
