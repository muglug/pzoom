<?php
class A {
    private const CRITICAL_ERRORS = [
        "category" => [],
        "name" => [],
        "geo" => [],
        "city" => [],
        "url" => [],
        "comment_critical" => [],
        "place" => [],
        "price" => [],
        "robot_error" => [],
        "manual" => [],
        "contacts" => [],
        "not_confirmed_by_other_source" => [],
    ];


    public function isCriticalError(int|string $key): bool {
        if (!\array_key_exists($key, A::CRITICAL_ERRORS)) {
            return false;
        }

        return true;
    }
}
