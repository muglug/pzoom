<?php
class a {
    const STATE_A = 0;
    const STATE_B = 1;
    const STATE_C = 2;
    /**
     * @return array<self::STATE_*, non-empty-string>
     * @psalm-pure
     */
    public static function getStateLabels(): array {
        return [
            self::STATE_A => "A",
            self::STATE_B => "B",
            self::STATE_C => "C",
        ];
    }
    /**
     * @param self::STATE_* $state
     */
    public static function getStateLabelIf(int $state): string {
        $states = self::getStateLabels();
        if (array_key_exists($state, $states)) {
            return $states[$state];
        }
        return "";
    }
}
