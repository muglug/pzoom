<?php
interface ReturnsString {
    public function getId(): string;
}

/**
 * @param mixed $a
 */
function foo(ReturnsString $user, $a): string {
    strlen($user->getId());

    (is_object($a) && method_exists($a, "getS")) ? (string)$a->GETS() : "";

    return $user->getId();
}
