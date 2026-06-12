<?php
final class Table7 { public function render(): void {} }

final class CompactR {
    /**
     * @psalm-suppress PossiblyNullReference
     */
    /** @param list<int> $items */
    public function create(array $items): void {
        /** @var Table7|null $table */
        $table = null;
        /** @var string|null $buffer */
        $buffer = null;

        foreach ($items as $_i) {
            if ($buffer !== null) {
                $table->render();
            }
            $buffer = 'b';
            $table = new Table7();
        }
        if ($buffer !== null) {
            $table->render();
        }
    }
}
