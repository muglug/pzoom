<?php
class CodeLocation {}
class Storage { public ?CodeLocation $location = null; }

class Codebase {
    /**
     * @return ($location is null ? int : int|null)
     */
    public function getOrRegisterTaint(string $taint_type, ?CodeLocation $location = null): ?int
    {
        if ($location !== null && rand(0,1)) {
            return null;
        }
        return 1;
    }
}

function f(Codebase $codebase, Storage $storage, string $s): int {
    $acc = 0;
    $taint = $codebase->getOrRegisterTaint($s, $storage->location);
    if ($taint !== null) {
        $acc |= $taint;
    }
    return $acc;
}
