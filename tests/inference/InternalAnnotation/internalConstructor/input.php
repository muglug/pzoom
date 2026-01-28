<?php
                    namespace A {
                        class C {
                            /** @internal */
                            public function __construct() {}
                        }
                    }
                    namespace B {
                        use A\C;
                        new C;
                    }
