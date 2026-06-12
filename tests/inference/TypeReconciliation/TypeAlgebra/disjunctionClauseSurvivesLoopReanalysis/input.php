<?php

class MemberRegistry {
    public function addMixedMemberName(string $key, string $source): void {
        echo $key . $source;
    }
}

class AtomicPart { public string $value = ''; }

/**
 * @param list<AtomicPart> $atomics
 */
function recordMixedNames(
    array $atomics,
    ?MemberRegistry $registry = null,
    ?string $calling_method_id = null,
    ?string $file_name = null
): void {
    if ($registry && ($calling_method_id || $file_name)) {
        foreach ($atomics as $atomic) {
            $registry->addMixedMemberName(
                strtolower($atomic->value) . '::',
                $calling_method_id ?: $file_name,
            );
        }
    }
}

/** @param array{type?: string} $node */
function loopCarriedConditionalWrite(array $node, bool $cloned): bool {
    foreach (['type', 'signature_type'] as $key) {
        if (!isset($node[$key])) {
            continue;
        }
        if (!$cloned) {
            $cloned = true;
        }
        echo $node[$key] ?? '';
    }
    return $cloned;
}
