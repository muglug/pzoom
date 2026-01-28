<?php
class pony
{
}
/**
 * @param array{0?: string, test?: string, pony?: string} $params
 * @return string|null
 */
function a(array $params = [])
{
    foreach ([0, "test", pony::class] as $key) {
        if (\array_key_exists($key, $params)) {
            return $params[$key];
        }
    }
}