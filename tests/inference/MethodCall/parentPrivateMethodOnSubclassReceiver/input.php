<?php
abstract class Analyzer {
    /** @param list<int> $positions */
    public function check(array $positions): void {
        $pos = $this->prevPos(0);
        foreach ($positions as $position) {
            if ($position !== $pos) {
                break;
            }
            $pos = $this->prevPos($position);
            if ($this instanceof ClosureLike) {
                continue;
            }
            echo $pos;
        }

        $other = $this instanceof ClosureLike ? $this : null;
        if ($other !== null) {
            echo $other->prevPos(1);
        }
    }

    private function prevPos(int $p): int { return $p - 1; }
}

final class ClosureLike extends Analyzer {}
