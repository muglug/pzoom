<?php
trait T {
    public function localChecks(): void {
        $x = strlen([]);
        strlen("a", "b");
    }
}
final class C { use T; }
