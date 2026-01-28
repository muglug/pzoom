<?php
/**
 * @template T
 */
abstract class C {
    /** @param T $t */
    public function foo($t): void {
        if (!$t) {}
        if ($t) {}
     }
}