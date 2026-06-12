<?php
class N { /** @return list<string> */ public function getParts(): array { return []; } }
class I2 { public string $name = ""; }
class CCF { public N|string $class = ""; public I2|string $name = ""; }
class PF { public CCF|string $var = ""; public I2|string $name = ""; }

function f(PF $stmt, ?string $fq, ?string $parent_fq): void {
    if ($stmt->var instanceof CCF
        && $stmt->var->class instanceof N
        && $stmt->var->name instanceof I2
        && $stmt->name instanceof I2
        && in_array($stmt->name->name, ['name', 'value'], true)
        && ($stmt->var->class->getParts() !== ['self'] || $fq !== null)
        && $stmt->var->class->getParts() !== ['static']
        && ($stmt->var->class->getParts() !== ['parent'] || $parent_fq !== null)
    ) {
        if ($stmt->var->class->getParts() === ['self']) {
            assert($fq !== null);
            echo $fq;
        } else {
            if ($stmt->var->class->getParts() === ['parent']) {
                assert($parent_fq !== null);
                echo $parent_fq;
            }
        }
    }
}
