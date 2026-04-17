#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    /// Push the null reference onto the operand stack.
    ///
    /// Actual use: initializes reference locals or passes a null reference explicitly.
    AconstNull = 0x01,
    /// Push int constant `-1` onto the operand stack.
    ///
    /// Actual use: a compact way to materialize `-1` without reading extra bytes.
    /// Java compilers often use these short `iconst_*` forms for tiny integers.
    IconstM1 = 0x02,
    /// Push int constant `0` onto the operand stack.
    ///
    /// Actual use: initializes counters, zero values, and false-like integer results.
    Iconst0 = 0x03,
    /// Push int constant `1` onto the operand stack.
    ///
    /// Actual use: common in increments, boolean-like true values, and loop steps.
    Iconst1 = 0x04,
    /// Push int constant `2` onto the operand stack.
    ///
    /// Actual use: small literal arithmetic and compact constant loading.
    Iconst2 = 0x05,
    /// Push int constant `3` onto the operand stack.
    ///
    /// Actual use: same role as other `iconst_*` instructions for frequent tiny literals.
    Iconst3 = 0x06,
    /// Push int constant `4` onto the operand stack.
    ///
    /// Actual use: avoids a wider push instruction for a very small integer literal.
    Iconst4 = 0x07,
    /// Push int constant `5` onto the operand stack.
    ///
    /// Actual use: the largest dedicated `iconst_*` literal; larger values move to
    /// `bipush`, `sipush`, or constant-pool-backed loading.
    Iconst5 = 0x08,
    /// Push long constant 0.
    Lconst0 = 0x09,
    /// Push long constant 1.
    Lconst1 = 0x0a,
    /// Push float constant 0.0.
    Fconst0 = 0x0b,
    /// Push float constant 1.0.
    Fconst1 = 0x0c,
    /// Push float constant 2.0.
    Fconst2 = 0x0d,
    /// Push double constant 0.0.
    Dconst0 = 0x0e,
    /// Push double constant 1.0.
    Dconst1 = 0x0f,
    /// Push a signed 8-bit immediate integer onto the operand stack.
    ///
    /// Actual use: compact loading for literals in the `-128..=127` range when no
    /// dedicated `iconst_*` form exists.
    Bipush = 0x10,
    /// Push a signed 16-bit immediate integer onto the operand stack.
    ///
    /// Actual use: loads medium-sized integer literals without going through `ldc`.
    Sipush = 0x11,
    /// Load a constant from the method's constant area by one-byte index.
    ///
    /// Actual use: in a full JVM this usually reads from the class file constant pool.
    /// In this project it reads from `Method.constants`.
    Ldc = 0x12,
    /// Load a constant by two-byte wide index.
    LdcW = 0x13,
    /// Load a long or double constant by two-byte wide index (category 2).
    Ldc2W = 0x14,
    /// Load an int from a local-variable slot specified by the next byte.
    ///
    /// Actual use: moves a method-local int into the operand stack so arithmetic or
    /// branching instructions can consume it.
    Iload = 0x15,
    /// Load a long from a local-variable slot specified by the next byte.
    Lload = 0x16,
    /// Load a float from a local-variable slot specified by the next byte.
    Fload = 0x17,
    /// Load a double from a local-variable slot specified by the next byte.
    Dload = 0x18,
    /// Load a reference from a local-variable slot specified by the next byte.
    ///
    /// Actual use: moves arrays or objects from locals onto the operand stack.
    Aload = 0x19,
    /// Load the int in local slot `0` onto the operand stack.
    ///
    /// Actual use: shorthand for `iload 0`, often used for the first local or first
    /// method argument in static methods.
    Iload0 = 0x1a,
    /// Load the int in local slot `1` onto the operand stack.
    ///
    /// Actual use: shorthand for `iload 1`, avoiding an extra index byte.
    Iload1 = 0x1b,
    /// Load the int in local slot `2` onto the operand stack.
    ///
    /// Actual use: shorthand for `iload 2`.
    Iload2 = 0x1c,
    /// Load the int in local slot `3` onto the operand stack.
    ///
    /// Actual use: shorthand for `iload 3`.
    Iload3 = 0x1d,
    Lload0 = 0x1e,
    Lload1 = 0x1f,
    Lload2 = 0x20,
    Lload3 = 0x21,
    Fload0 = 0x22,
    Fload1 = 0x23,
    Fload2 = 0x24,
    Fload3 = 0x25,
    Dload0 = 0x26,
    Dload1 = 0x27,
    Dload2 = 0x28,
    Dload3 = 0x29,
    /// Load the reference in local slot `0` onto the operand stack.
    ///
    /// Actual use: shorthand for `aload 0`; static `main(String[] args)` receives `args` here.
    Aload0 = 0x2a,
    /// Load the reference in local slot `1` onto the operand stack.
    ///
    /// Actual use: shorthand for `aload 1`.
    Aload1 = 0x2b,
    /// Load the reference in local slot `2` onto the operand stack.
    ///
    /// Actual use: shorthand for `aload 2`.
    Aload2 = 0x2c,
    /// Load the reference in local slot `3` onto the operand stack.
    ///
    /// Actual use: shorthand for `aload 3`.
    Aload3 = 0x2d,
    /// Load an int from an int array.
    ///
    /// Actual use: backs expressions like `numbers[i]` for `int[]`.
    Iaload = 0x2e,
    /// Load a long from a long array.
    Laload = 0x2f,
    /// Load a float from a float array.
    Faload = 0x30,
    /// Load a double from a double array.
    Daload = 0x31,
    /// Load a reference from a reference array.
    ///
    /// Actual use: backs expressions like `args[i]`.
    Aaload = 0x32,
    /// Load a byte or boolean from an array.
    Baload = 0x33,
    /// Load a char from a char array.
    Caload = 0x34,
    /// Load a short from a short array.
    Saload = 0x35,
    /// Store the top int from the operand stack into the local slot named by the next byte.
    ///
    /// Actual use: saves temporary arithmetic results or method arguments for later reuse.
    Istore = 0x36,
    /// Store a long into a local-variable slot specified by the next byte.
    Lstore = 0x37,
    /// Store a float into a local-variable slot specified by the next byte.
    Fstore = 0x38,
    /// Store a double into a local-variable slot specified by the next byte.
    Dstore = 0x39,
    /// Store the top reference from the operand stack into the local slot named by the next byte.
    ///
    /// Actual use: saves arrays or objects into a local variable.
    Astore = 0x3a,
    /// Store the top int into local slot `0`.
    ///
    /// Actual use: shorthand for `istore 0`.
    Istore0 = 0x3b,
    /// Store the top int into local slot `1`.
    ///
    /// Actual use: shorthand for `istore 1`.
    Istore1 = 0x3c,
    /// Store the top int into local slot `2`.
    ///
    /// Actual use: shorthand for `istore 2`.
    Istore2 = 0x3d,
    /// Store the top int into local slot `3`.
    ///
    /// Actual use: shorthand for `istore 3`.
    Istore3 = 0x3e,
    Lstore0 = 0x3f,
    Lstore1 = 0x40,
    Lstore2 = 0x41,
    Lstore3 = 0x42,
    Fstore0 = 0x43,
    Fstore1 = 0x44,
    Fstore2 = 0x45,
    Fstore3 = 0x46,
    Dstore0 = 0x47,
    Dstore1 = 0x48,
    Dstore2 = 0x49,
    Dstore3 = 0x4a,
    /// Store the top reference into local slot `0`.
    ///
    /// Actual use: shorthand for `astore 0`.
    Astore0 = 0x4b,
    /// Store the top reference into local slot `1`.
    ///
    /// Actual use: shorthand for `astore 1`.
    Astore1 = 0x4c,
    /// Store the top reference into local slot `2`.
    ///
    /// Actual use: shorthand for `astore 2`.
    Astore2 = 0x4d,
    /// Store the top reference into local slot `3`.
    ///
    /// Actual use: shorthand for `astore 3`.
    Astore3 = 0x4e,
    /// Store an int into an int array.
    ///
    /// Actual use: backs statements like `numbers[i] = value`.
    Iastore = 0x4f,
    /// Store a long into a long array.
    Lastore = 0x50,
    /// Store a float into a float array.
    Fastore = 0x51,
    /// Store a double into a double array.
    Dastore = 0x52,
    /// Store a reference into a reference array.
    ///
    /// Actual use: backs statements like `array[i] = value`.
    Aastore = 0x53,
    /// Store a byte or boolean into an array.
    Bastore = 0x54,
    /// Store a char into a char array.
    Castore = 0x55,
    /// Store a short into a short array.
    Sastore = 0x56,
    /// Discard the top value on the operand stack.
    ///
    /// Actual use: throws away an unused result, common after calculations or calls whose
    /// value is not needed.
    Pop = 0x57,
    /// Discard the top one or two values from the operand stack.
    Pop2 = 0x58,
    /// Duplicate the top value on the operand stack.
    ///
    /// Actual use: reuse one computed value twice without reloading it, such as storing it
    /// and still keeping a copy for a following operation.
    Dup = 0x59,
    /// Duplicate the top value and insert it one position down.
    ///
    /// Actual use: stack reshaping for expressions where one freshly computed value must be
    /// reused while preserving the previous top element underneath it.
    DupX1 = 0x5a,
    /// Duplicate the top value and insert it two positions down.
    DupX2 = 0x5b,
    /// Duplicate the top two category-1 values.
    ///
    /// Actual use: reuse a pair of ints without reloading them from locals.
    Dup2 = 0x5c,
    /// Duplicate the top two values and insert them one position down.
    Dup2X1 = 0x5d,
    /// Duplicate the top two values and insert them two positions down.
    Dup2X2 = 0x5e,
    /// Swap the top two category-1 values.
    ///
    /// Actual use: reorder operands after evaluation order has pushed them in the wrong shape
    /// for the next bytecode sequence.
    Swap = 0x5f,
    /// Pop two ints, add them, and push the result.
    ///
    /// Actual use: integer arithmetic expressions like `a + b`.
    Iadd = 0x60,
    Ladd = 0x61,
    Fadd = 0x62,
    Dadd = 0x63,
    /// Pop two ints, subtract the right operand from the left, and push the result.
    ///
    /// Actual use: integer arithmetic expressions like `a - b`.
    Isub = 0x64,
    Lsub = 0x65,
    Fsub = 0x66,
    Dsub = 0x67,
    /// Pop two ints, multiply them, and push the result.
    ///
    /// Actual use: integer arithmetic expressions like `a * b`.
    Imul = 0x68,
    Lmul = 0x69,
    Fmul = 0x6a,
    Dmul = 0x6b,
    /// Pop two ints, divide the left operand by the right, and push the result.
    ///
    /// Actual use: integer arithmetic expressions like `a / b`; division by zero raises an error.
    Idiv = 0x6c,
    Ldiv = 0x6d,
    Fdiv = 0x6e,
    Ddiv = 0x6f,
    /// Pop two ints, compute remainder, and push the result.
    ///
    /// Actual use: modulo-style expressions like `a % b`, often seen in parity checks or wrapping logic.
    Irem = 0x70,
    Lrem = 0x71,
    Frem = 0x72,
    Drem = 0x73,
    /// Negate the top int on the operand stack.
    ///
    /// Actual use: unary minus expressions like `-x`.
    Ineg = 0x74,
    Lneg = 0x75,
    Fneg = 0x76,
    Dneg = 0x77,
    /// Shift int left.
    Ishl = 0x78,
    Lshl = 0x79,
    /// Arithmetic shift int right.
    Ishr = 0x7a,
    Lshr = 0x7b,
    /// Logical shift int right.
    Iushr = 0x7c,
    Lushr = 0x7d,
    /// Bitwise AND of two ints.
    Iand = 0x7e,
    Land = 0x7f,
    /// Bitwise OR of two ints.
    Ior = 0x80,
    Lor = 0x81,
    /// Bitwise XOR of two ints.
    Ixor = 0x82,
    Lxor = 0x83,
    /// Increment a local int variable by a signed 8-bit immediate.
    ///
    /// Actual use: heavily used by Java compilers for loop counters like `i++` and `i += step`.
    Iinc = 0x84,
    /// Int to long.
    I2l = 0x85,
    /// Int to float.
    I2f = 0x86,
    /// Int to double.
    I2d = 0x87,
    /// Long to int.
    L2i = 0x88,
    /// Long to float.
    L2f = 0x89,
    /// Long to double.
    L2d = 0x8a,
    /// Float to int.
    F2i = 0x8b,
    /// Float to long.
    F2l = 0x8c,
    /// Float to double.
    F2d = 0x8d,
    /// Double to int.
    D2i = 0x8e,
    /// Double to long.
    D2l = 0x8f,
    /// Double to float.
    D2f = 0x90,
    /// Narrow int to byte.
    I2b = 0x91,
    /// Narrow int to char (unsigned 16-bit).
    I2c = 0x92,
    /// Narrow int to short.
    I2s = 0x93,
    /// Compare two longs; push -1, 0, or 1.
    Lcmp = 0x94,
    /// Compare two floats; push -1, 0, or 1 (NaN → -1).
    Fcmpl = 0x95,
    /// Compare two floats; push -1, 0, or 1 (NaN → 1).
    Fcmpg = 0x96,
    /// Compare two doubles; push -1, 0, or 1 (NaN → -1).
    Dcmpl = 0x97,
    /// Compare two doubles; push -1, 0, or 1 (NaN → 1).
    Dcmpg = 0x98,
    /// Pop one int; branch if it equals zero.
    ///
    /// Actual use: the low-level building block for `if (x == 0)` and many compiler-generated
    /// boolean/control-flow patterns.
    Ifeq = 0x99,
    /// Pop one int; branch if it is not zero.
    ///
    /// Actual use: the low-level building block for `if (x != 0)` and truthy/non-zero tests.
    Ifne = 0x9a,
    /// Pop one int; branch if it is less than zero.
    ///
    /// Actual use: backs comparisons like `if (x < 0)`.
    Iflt = 0x9b,
    /// Pop one int; branch if it is greater than or equal to zero.
    ///
    /// Actual use: backs comparisons like `if (x >= 0)`.
    Ifge = 0x9c,
    /// Pop one int; branch if it is greater than zero.
    ///
    /// Actual use: backs comparisons like `if (x > 0)`.
    Ifgt = 0x9d,
    /// Pop one int; branch if it is less than or equal to zero.
    ///
    /// Actual use: backs comparisons like `if (x <= 0)`.
    Ifle = 0x9e,
    /// Pop two ints; branch if they are equal.
    ///
    /// Actual use: compiled comparisons like `if (a == b)`.
    IfIcmpeq = 0x9f,
    /// Pop two ints; branch if they are not equal.
    ///
    /// Actual use: compiled comparisons like `if (a != b)`.
    IfIcmpne = 0xa0,
    /// Pop two ints; branch if the left operand is less than the right operand.
    ///
    /// Actual use: backs comparisons like `if (a < b)`.
    IfIcmplt = 0xa1,
    /// Pop two ints; branch if the left operand is greater than or equal to the right operand.
    ///
    /// Actual use: backs comparisons like `if (a >= b)`.
    IfIcmpge = 0xa2,
    /// Pop two ints; branch if the left operand is greater than the right operand.
    ///
    /// Actual use: backs comparisons like `if (a > b)`.
    IfIcmpgt = 0xa3,
    /// Pop two ints; branch if the left operand is less than or equal to the right operand.
    ///
    /// Actual use: backs comparisons like `if (a <= b)`.
    IfIcmple = 0xa4,
    /// Pop two references; branch if they are the same reference.
    ///
    /// Actual use: backs reference identity checks like `if (a == b)`.
    IfAcmpeq = 0xa5,
    /// Pop two references; branch if they are different references.
    ///
    /// Actual use: backs reference identity checks like `if (a != b)`.
    IfAcmpne = 0xa6,
    /// Unconditionally jump to a bytecode offset.
    ///
    /// Actual use: forms loops, skips else branches, and connects basic blocks after conditional checks.
    Goto = 0xa7,
    /// Access jump table by index and jump.
    Tableswitch = 0xaa,
    /// Access jump table by key match and jump.
    Lookupswitch = 0xab,
    /// Return an int value from the current method.
    Ireturn = 0xac,
    /// Return a long value from the current method.
    Lreturn = 0xad,
    /// Return a float value from the current method.
    Freturn = 0xae,
    /// Return a double value from the current method.
    Dreturn = 0xaf,
    /// Return a reference value from the current method.
    ///
    /// Actual use: ends a reference-returning method.
    Areturn = 0xb0,
    /// Return from the current method without a value.
    ///
    /// Actual use: ends a `void` method.
    Return = 0xb1,
    /// Read a static field and push its value.
    ///
    /// Actual use: backs expressions like `System.out`.
    Getstatic = 0xb2,
    /// Set a static field value.
    ///
    /// Actual use: backs assignments like `MyClass.counter = 0`.
    Putstatic = 0xb3,
    /// Read an instance field and push its value.
    ///
    /// Actual use: backs expressions like `this.name`.
    Getfield = 0xb4,
    /// Set an instance field value.
    ///
    /// Actual use: backs assignments like `this.name = value`.
    Putfield = 0xb5,
    /// Invoke an instance method using virtual dispatch.
    ///
    /// Actual use: backs calls like `out.println(value)`.
    Invokevirtual = 0xb6,
    /// Invoke an instance method using exact (non-virtual) dispatch.
    ///
    /// Actual use: backs constructor calls like `<init>` and `super.method()`.
    Invokespecial = 0xb7,
    /// Invoke a static method.
    ///
    /// Actual use: backs calls like `Math.max(a, b)`.
    Invokestatic = 0xb8,
    /// Jump to a legacy subroutine and push the return address.
    Jsr = 0xa8,
    /// Return from a legacy subroutine using a local return address.
    Ret = 0xa9,
    /// Invoke an interface method.
    Invokeinterface = 0xb9,
    /// Invoke a dynamically-computed call site.
    Invokedynamic = 0xba,
    /// Create a new object instance.
    ///
    /// Actual use: backs expressions like `new MyClass()`.
    New = 0xbb,
    /// Create a new one-dimensional primitive array.
    ///
    /// Actual use: backs expressions like `new int[count]`.
    Newarray = 0xbc,
    /// Create a new one-dimensional array of references.
    ///
    /// Actual use: backs expressions like `new String[count]`.
    Anewarray = 0xbd,
    /// Push the length of an array reference.
    ///
    /// Actual use: backs expressions like `args.length`.
    Arraylength = 0xbe,
    /// Throw an exception or error.
    Athrow = 0xbf,
    /// Check whether an object is of a given type; throw ClassCastException if not.
    Checkcast = 0xc0,
    /// Test whether an object is an instance of a given type; push 0 or 1.
    Instanceof = 0xc1,
    /// Enter monitor for object (no-op in single-threaded implementation).
    Monitorenter = 0xc2,
    /// Exit monitor for object (no-op in single-threaded implementation).
    Monitorexit = 0xc3,
    /// Widen the index of the following local-variable instruction to 16 bits.
    Wide = 0xc4,
    /// Create a multi-dimensional array.
    Multianewarray = 0xc5,
    /// Branch if the popped reference is null.
    ///
    /// Actual use: backs null checks like `if (x == null)`.
    Ifnull = 0xc6,
    /// Branch if the popped reference is not null.
    ///
    /// Actual use: backs null checks like `if (x != null)`.
    Ifnonnull = 0xc7,
    /// Wide unconditional jump with 4-byte offset.
    GotoW = 0xc8,
    /// Wide legacy subroutine jump.
    JsrW = 0xc9,
}

impl Opcode {
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0x01 => Self::AconstNull,
            0x02 => Self::IconstM1,
            0x03 => Self::Iconst0,
            0x04 => Self::Iconst1,
            0x05 => Self::Iconst2,
            0x06 => Self::Iconst3,
            0x07 => Self::Iconst4,
            0x08 => Self::Iconst5,
            0x09 => Self::Lconst0,
            0x0a => Self::Lconst1,
            0x0b => Self::Fconst0,
            0x0c => Self::Fconst1,
            0x0d => Self::Fconst2,
            0x0e => Self::Dconst0,
            0x0f => Self::Dconst1,
            0x10 => Self::Bipush,
            0x11 => Self::Sipush,
            0x12 => Self::Ldc,
            0x13 => Self::LdcW,
            0x14 => Self::Ldc2W,
            0x15 => Self::Iload,
            0x16 => Self::Lload,
            0x17 => Self::Fload,
            0x18 => Self::Dload,
            0x19 => Self::Aload,
            0x1a => Self::Iload0,
            0x1b => Self::Iload1,
            0x1c => Self::Iload2,
            0x1d => Self::Iload3,
            0x1e => Self::Lload0,
            0x1f => Self::Lload1,
            0x20 => Self::Lload2,
            0x21 => Self::Lload3,
            0x22 => Self::Fload0,
            0x23 => Self::Fload1,
            0x24 => Self::Fload2,
            0x25 => Self::Fload3,
            0x26 => Self::Dload0,
            0x27 => Self::Dload1,
            0x28 => Self::Dload2,
            0x29 => Self::Dload3,
            0x2a => Self::Aload0,
            0x2b => Self::Aload1,
            0x2c => Self::Aload2,
            0x2d => Self::Aload3,
            0x2e => Self::Iaload,
            0x2f => Self::Laload,
            0x30 => Self::Faload,
            0x31 => Self::Daload,
            0x32 => Self::Aaload,
            0x33 => Self::Baload,
            0x34 => Self::Caload,
            0x35 => Self::Saload,
            0x36 => Self::Istore,
            0x37 => Self::Lstore,
            0x38 => Self::Fstore,
            0x39 => Self::Dstore,
            0x3a => Self::Astore,
            0x3b => Self::Istore0,
            0x3c => Self::Istore1,
            0x3d => Self::Istore2,
            0x3e => Self::Istore3,
            0x3f => Self::Lstore0,
            0x40 => Self::Lstore1,
            0x41 => Self::Lstore2,
            0x42 => Self::Lstore3,
            0x43 => Self::Fstore0,
            0x44 => Self::Fstore1,
            0x45 => Self::Fstore2,
            0x46 => Self::Fstore3,
            0x47 => Self::Dstore0,
            0x48 => Self::Dstore1,
            0x49 => Self::Dstore2,
            0x4a => Self::Dstore3,
            0x4b => Self::Astore0,
            0x4c => Self::Astore1,
            0x4d => Self::Astore2,
            0x4e => Self::Astore3,
            0x4f => Self::Iastore,
            0x50 => Self::Lastore,
            0x51 => Self::Fastore,
            0x52 => Self::Dastore,
            0x53 => Self::Aastore,
            0x54 => Self::Bastore,
            0x55 => Self::Castore,
            0x56 => Self::Sastore,
            0x57 => Self::Pop,
            0x58 => Self::Pop2,
            0x59 => Self::Dup,
            0x5a => Self::DupX1,
            0x5b => Self::DupX2,
            0x5c => Self::Dup2,
            0x5d => Self::Dup2X1,
            0x5e => Self::Dup2X2,
            0x5f => Self::Swap,
            0x60 => Self::Iadd,
            0x61 => Self::Ladd,
            0x62 => Self::Fadd,
            0x63 => Self::Dadd,
            0x64 => Self::Isub,
            0x65 => Self::Lsub,
            0x66 => Self::Fsub,
            0x67 => Self::Dsub,
            0x68 => Self::Imul,
            0x69 => Self::Lmul,
            0x6a => Self::Fmul,
            0x6b => Self::Dmul,
            0x6c => Self::Idiv,
            0x6d => Self::Ldiv,
            0x6e => Self::Fdiv,
            0x6f => Self::Ddiv,
            0x70 => Self::Irem,
            0x71 => Self::Lrem,
            0x72 => Self::Frem,
            0x73 => Self::Drem,
            0x74 => Self::Ineg,
            0x75 => Self::Lneg,
            0x76 => Self::Fneg,
            0x77 => Self::Dneg,
            0x78 => Self::Ishl,
            0x79 => Self::Lshl,
            0x7a => Self::Ishr,
            0x7b => Self::Lshr,
            0x7c => Self::Iushr,
            0x7d => Self::Lushr,
            0x7e => Self::Iand,
            0x7f => Self::Land,
            0x80 => Self::Ior,
            0x81 => Self::Lor,
            0x82 => Self::Ixor,
            0x83 => Self::Lxor,
            0x84 => Self::Iinc,
            0x85 => Self::I2l,
            0x86 => Self::I2f,
            0x87 => Self::I2d,
            0x88 => Self::L2i,
            0x89 => Self::L2f,
            0x8a => Self::L2d,
            0x8b => Self::F2i,
            0x8c => Self::F2l,
            0x8d => Self::F2d,
            0x8e => Self::D2i,
            0x8f => Self::D2l,
            0x90 => Self::D2f,
            0x91 => Self::I2b,
            0x92 => Self::I2c,
            0x93 => Self::I2s,
            0x94 => Self::Lcmp,
            0x95 => Self::Fcmpl,
            0x96 => Self::Fcmpg,
            0x97 => Self::Dcmpl,
            0x98 => Self::Dcmpg,
            0x99 => Self::Ifeq,
            0x9a => Self::Ifne,
            0x9b => Self::Iflt,
            0x9c => Self::Ifge,
            0x9d => Self::Ifgt,
            0x9e => Self::Ifle,
            0x9f => Self::IfIcmpeq,
            0xa0 => Self::IfIcmpne,
            0xa1 => Self::IfIcmplt,
            0xa2 => Self::IfIcmpge,
            0xa3 => Self::IfIcmpgt,
            0xa4 => Self::IfIcmple,
            0xa5 => Self::IfAcmpeq,
            0xa6 => Self::IfAcmpne,
            0xa7 => Self::Goto,
            0xa8 => Self::Jsr,
            0xa9 => Self::Ret,
            0xaa => Self::Tableswitch,
            0xab => Self::Lookupswitch,
            0xac => Self::Ireturn,
            0xad => Self::Lreturn,
            0xae => Self::Freturn,
            0xaf => Self::Dreturn,
            0xb0 => Self::Areturn,
            0xb1 => Self::Return,
            0xb2 => Self::Getstatic,
            0xb3 => Self::Putstatic,
            0xb4 => Self::Getfield,
            0xb5 => Self::Putfield,
            0xb6 => Self::Invokevirtual,
            0xb7 => Self::Invokespecial,
            0xb8 => Self::Invokestatic,
            0xb9 => Self::Invokeinterface,
            0xba => Self::Invokedynamic,
            0xbb => Self::New,
            0xbc => Self::Newarray,
            0xbd => Self::Anewarray,
            0xbe => Self::Arraylength,
            0xbf => Self::Athrow,
            0xc0 => Self::Checkcast,
            0xc1 => Self::Instanceof,
            0xc2 => Self::Monitorenter,
            0xc3 => Self::Monitorexit,
            0xc4 => Self::Wide,
            0xc5 => Self::Multianewarray,
            0xc6 => Self::Ifnull,
            0xc7 => Self::Ifnonnull,
            0xc8 => Self::GotoW,
            0xc9 => Self::JsrW,
            _ => return None,
        })
    }
}
