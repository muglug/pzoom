<?php
trait T {
    public function badReturn(): int {
        return "nope";
    }
}
final class C { use T; }
