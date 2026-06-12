<?php
namespace A {
    class C { public function f(): void {} }
}
namespace B {
    use A\C;
    /** @psalm-scope-this C */
    ?>
    <h1><?php $this->f(); ?></h1>
    <?php
}
