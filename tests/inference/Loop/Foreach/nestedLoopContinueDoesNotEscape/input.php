<?php

class U3 {}
class Atomic3 {}
class TT extends Atomic3 {
    public string $param_name = 'x';
}

abstract class Holder {
    /** @return non-empty-array<string, Atomic3> */
    abstract public function getAtomicTypes(): array;
}

function combine(?U3 $a, ?U3 $b): U3
{
    return $a ?? $b ?? new U3();
}

/** @param non-empty-list<U3> $arr */
function combineArray(array $arr): U3
{
    return $arr[0];
}

function replace3(U3 $u): U3 { return $u; }

/**
 * @param array<string, Holder> $params
 * @param array<string, array<string, U3>> $template_types
 * @return list<U3>
 */
function f(array $params, array $template_types): array
{
    $new_input_params = [];
    foreach ($params as $holder) {
        $new_input_param = null;

        foreach ($holder->getAtomicTypes() as $extended_template) {
            $extended_templates = $extended_template instanceof TT ? [$extended_template] : [];

            $candidate_param_types = [];

            if ($extended_templates) {
                foreach ($extended_templates as $template) {
                    if (!isset($template_types[$template->param_name]['c'])) {
                        continue;
                    }
                    $candidate_param_types[] = $template_types[$template->param_name]['c'];
                }
            }

            $new_input_param = combine(
                $new_input_param,
                $candidate_param_types ? combineArray($candidate_param_types) : new U3(),
            );
        }

        $new_input_param = replace3($new_input_param);

        $new_input_params[] = $new_input_param;
    }
    return $new_input_params;
}
