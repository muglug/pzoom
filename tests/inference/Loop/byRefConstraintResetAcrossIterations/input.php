<?php
abstract class Atomic3 {}
final class TCC extends Atomic3 {}

final class Rule {
    public function getAtomicType(): ?Atomic3 { return null; }
}

final class Expander {
    /**
     * @param-out Atomic3 $return_type
     * @return non-empty-list<Atomic3>
     */
    public static function expandAtomic(Atomic3 &$return_type): array {
        return [$return_type];
    }
}

/** @param list<list<Rule>> $groups */
function f(array $groups): void {
    foreach ($groups as $rules) {
        foreach ($rules as $rule) {
            $rule_type = $rule->getAtomicType();

            if ($rule_type instanceof TCC) {
                echo count(Expander::expandAtomic($rule_type));
            }
        }
    }
}
