//! Contents of the Mathcad 15 math palettes (Calculator, Graph, Matrix, ...).
//! `■` marks placeholders; Tab jumps between them.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Palette {
    Calculator,
    Graph,
    Matrix,
    Evaluation,
    Calculus,
    Boolean,
    Programming,
    Greek,
    Symbolic,
}

impl Palette {
    pub const ALL: [Palette; 9] = [
        Palette::Calculator,
        Palette::Graph,
        Palette::Matrix,
        Palette::Evaluation,
        Palette::Calculus,
        Palette::Boolean,
        Palette::Programming,
        Palette::Greek,
        Palette::Symbolic,
    ];

    /// Window title (English, as in Mathcad 15) and menu name.
    pub fn title(self) -> &'static str {
        match self {
            Palette::Calculator => "Calculator",
            Palette::Graph => "Graph",
            Palette::Matrix => "Matrix",
            Palette::Evaluation => "Evaluation",
            Palette::Calculus => "Calculus",
            Palette::Boolean => "Boolean",
            Palette::Programming => "Programming",
            Palette::Greek => "Greek",
            Palette::Symbolic => "Symbolic",
        }
    }

    pub fn menu_name(self) -> &'static str {
        match self {
            Palette::Calculator => "Калькулятор",
            Palette::Graph => "Графики",
            Palette::Matrix => "Векторы и матрицы",
            Palette::Evaluation => "Вычисления",
            Palette::Calculus => "Математический анализ",
            Palette::Boolean => "Булевы операторы",
            Palette::Programming => "Программирование",
            Palette::Greek => "Греческие буквы",
            Palette::Symbolic => "Символьные",
        }
    }

    pub fn columns(self) -> usize {
        match self {
            Palette::Calculator => 5,
            Palette::Graph => 3,
            Palette::Matrix => 4,
            Palette::Evaluation => 4,
            Palette::Calculus => 4,
            Palette::Boolean => 4,
            Palette::Programming => 2,
            Palette::Greek => 8,
            Palette::Symbolic => 3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Act {
    /// Insert text at the cursor (or start a new math region).
    Insert(&'static str),
    InsertPlot,
    MatrixDialog,
    /// Not implemented yet; shown disabled.
    Todo,
}

pub struct Btn {
    pub label: &'static str,
    pub tip: &'static str,
    pub act: Act,
}

const fn b(label: &'static str, tip: &'static str, act: Act) -> Btn {
    Btn { label, tip, act }
}

use Act::*;

pub static CALCULATOR: &[Btn] = &[
    b("sin", "Синус", Insert("sin(■)")),
    b("cos", "Косинус", Insert("cos(■)")),
    b("tan", "Тангенс", Insert("tan(■)")),
    b("ln", "Натуральный логарифм", Insert("ln(■)")),
    b("log", "Десятичный логарифм", Insert("log(■)")),
    b("n!", "Факториал  !", Insert("■!")),
    b("i", "Мнимая единица", Insert("1i")),
    b("|x|", "Модуль  |", Insert("|■|")),
    b("√", "Квадратный корень  \\", Insert("sqrt(■)")),
    b("ⁿ√", "Корень n-й степени  Ctrl+\\", Insert("nthroot(■, ■)")),
    b("eˣ", "Экспонента", Insert("exp(■)")),
    b("1/x", "Обратная величина", Insert("1/■")),
    b("( )", "Скобки", Insert("(■)")),
    b("x²", "Квадрат", Insert("^2")),
    b("xʸ", "Степень  ^", Insert("^■")),
    b("π", "Пи  Ctrl+Shift+P", Insert("π")),
    b("7", "", Insert("7")),
    b("8", "", Insert("8")),
    b("9", "", Insert("9")),
    b("/", "Деление  /", Insert("/")),
    b("■/■", "Дробь", Insert("■/■")),
    b("4", "", Insert("4")),
    b("5", "", Insert("5")),
    b("6", "", Insert("6")),
    b("×", "Умножение  *", Insert("*")),
    b("÷", "Деление в строку", Insert("/")),
    b("1", "", Insert("1")),
    b("2", "", Insert("2")),
    b("3", "", Insert("3")),
    b("+", "Сложение  +", Insert("+")),
    b(":=", "Определение  :", Insert(":=")),
    b(".", "Десятичная точка", Insert(".")),
    b("0", "", Insert("0")),
    b("−", "Вычитание  -", Insert("-")),
    b("=", "Вычислить  =", Insert("=")),
];

pub static GRAPH: &[Btn] = &[
    b("X-Y", "Декартов график  @", InsertPlot),
    b("Полярн.", "Полярный график  Ctrl+7", Todo),
    b("Поверхн.", "График поверхности  Ctrl+2", Todo),
    b("Контур", "Контурный график  Ctrl+5", Todo),
    b("3D бар", "3D гистограмма", Todo),
    b("3D точ.", "3D точечный график", Todo),
    b("Вектор", "Векторное поле", Todo),
    b("Масштаб", "Масштаб", Todo),
    b("Трасс.", "Трассировка", Todo),
];

pub static MATRIX: &[Btn] = &[
    b("[:::]", "Матрица или вектор  Ctrl+M", MatrixDialog),
    b("xₙ", "Нижний индекс  [", Insert("[■]")),
    b("x⁻¹", "Обратная матрица", Insert("^-1")),
    b("|x|", "Определитель  |", Insert("|■|")),
    b("f(M)⃗", "Векторизация  Ctrl+-", Todo),
    b("M⟨⟩", "Столбец матрицы  Ctrl+6", Todo),
    b("Mᵀ", "Транспонирование  Ctrl+1", Insert("transpose(■)")),
    b("m..n", "Ранжированная переменная  ;", Insert("..")),
    b("x·y", "Скалярное произведение  *", Insert("*")),
    b("x×y", "Векторное произведение  Ctrl+8", Insert("cross(■, ■)")),
    b("Σv", "Сумма элементов вектора  Alt+4", Insert("sum(■)")),
    b("det", "Определитель (функция)", Insert("det(■)")),
];

pub static EVALUATION: &[Btn] = &[
    b("=", "Вычислить  =", Insert("=")),
    b(":=", "Определение  :", Insert(":=")),
    b("≡", "Глобальное определение  ~", Insert("≡")),
    b("→", "Символьное вычисление  Ctrl+.", Todo),
    b("•→", "Символьное с ключевым словом", Todo),
    b("fx", "Префиксный оператор", Todo),
    b("xf", "Постфиксный оператор", Todo),
    b("xfy", "Инфиксный оператор", Todo),
];

pub static CALCULUS: &[Btn] = &[
    b("d/dx", "Производная  ?", Insert("diff(■, ■)")),
    b("dⁿ/dxⁿ", "n-я производная  Ctrl+?", Todo),
    b("∞", "Бесконечность  Ctrl+Shift+Z", Insert("∞")),
    b("∫ₐᵇ", "Определённый интеграл  &", Insert("integrate(■, ■, ■, ■)")),
    b("Σₙ₌", "Сумма с пределами  Ctrl+Shift+4", Insert("sum(■, ■, ■, ■)")),
    b("Πₙ₌", "Произведение с пределами  Ctrl+Shift+3", Insert("prod(■, ■, ■, ■)")),
    b("∫", "Неопределённый интеграл  Ctrl+I", Todo),
    b("Σₙ", "Сумма по ранжированной переменной  $", Insert("sum(■, ■)")),
    b("Πₙ", "Произведение по ранжированной переменной  #", Insert("prod(■, ■)")),
    b("lim", "Предел  Ctrl+L", Todo),
    b("lim+", "Предел справа", Todo),
    b("lim−", "Предел слева", Todo),
    b("∇", "Градиент", Todo),
];

pub static BOOLEAN: &[Btn] = &[
    b("=", "Равно  Ctrl+=", Insert("==")),
    b("<", "Меньше", Insert("<")),
    b(">", "Больше", Insert(">")),
    b("≤", "Меньше или равно  Ctrl+9", Insert("≤")),
    b("≥", "Больше или равно  Ctrl+0", Insert("≥")),
    b("≠", "Не равно  Ctrl+3", Insert("≠")),
    b("¬", "Логическое НЕ  Ctrl+Shift+1", Insert("¬")),
    b("∧", "Логическое И  Ctrl+Shift+7", Insert("∧")),
    b("∨", "Логическое ИЛИ  Ctrl+Shift+6", Insert("∨")),
    b("⊕", "Исключающее ИЛИ", Todo),
];

pub static PROGRAMMING: &[Btn] = &[
    b("Add Line", "Добавить строку программы  ]", Todo),
    b("←", "Локальное присваивание  {", Todo),
    b("if", "Условие (функция if)", Insert("if(■, ■, ■)")),
    b("otherwise", "Иначе", Todo),
    b("for", "Цикл for", Todo),
    b("while", "Цикл while", Todo),
    b("break", "Прервать цикл", Todo),
    b("continue", "Продолжить цикл", Todo),
    b("return", "Возврат", Todo),
    b("on error", "Обработка ошибки", Todo),
];

pub static GREEK: &[Btn] = &[
    b("α", "", Insert("α")),
    b("β", "", Insert("β")),
    b("γ", "", Insert("γ")),
    b("δ", "", Insert("δ")),
    b("ε", "", Insert("ε")),
    b("ζ", "", Insert("ζ")),
    b("η", "", Insert("η")),
    b("θ", "", Insert("θ")),
    b("ι", "", Insert("ι")),
    b("κ", "", Insert("κ")),
    b("λ", "", Insert("λ")),
    b("μ", "", Insert("μ")),
    b("ν", "", Insert("ν")),
    b("ξ", "", Insert("ξ")),
    b("ο", "", Insert("ο")),
    b("π", "", Insert("π")),
    b("ρ", "", Insert("ρ")),
    b("σ", "", Insert("σ")),
    b("τ", "", Insert("τ")),
    b("υ", "", Insert("υ")),
    b("φ", "", Insert("φ")),
    b("χ", "", Insert("χ")),
    b("ψ", "", Insert("ψ")),
    b("ω", "", Insert("ω")),
    b("Α", "", Insert("Α")),
    b("Β", "", Insert("Β")),
    b("Γ", "", Insert("Γ")),
    b("Δ", "", Insert("Δ")),
    b("Ε", "", Insert("Ε")),
    b("Ζ", "", Insert("Ζ")),
    b("Η", "", Insert("Η")),
    b("Θ", "", Insert("Θ")),
    b("Ι", "", Insert("Ι")),
    b("Κ", "", Insert("Κ")),
    b("Λ", "", Insert("Λ")),
    b("Μ", "", Insert("Μ")),
    b("Ν", "", Insert("Ν")),
    b("Ξ", "", Insert("Ξ")),
    b("Ο", "", Insert("Ο")),
    b("Π", "", Insert("Π")),
    b("Ρ", "", Insert("Ρ")),
    b("Σ", "", Insert("Σ")),
    b("Τ", "", Insert("Τ")),
    b("Υ", "", Insert("Υ")),
    b("Φ", "", Insert("Φ")),
    b("Χ", "", Insert("Χ")),
    b("Ψ", "", Insert("Ψ")),
    b("Ω", "", Insert("Ω")),
];

pub static SYMBOLIC: &[Btn] = &[
    b("→", "Символьное вычисление", Todo),
    b("•→", "С ключевым словом", Todo),
    b("Modifiers", "Модификаторы", Todo),
    b("float", "Число с плавающей точкой", Todo),
    b("rectangular", "Прямоугольная форма", Todo),
    b("assume", "Допущения", Todo),
    b("solve", "Решить", Todo),
    b("simplify", "Упростить", Todo),
    b("substitute", "Подставить", Todo),
    b("factor", "Разложить на множители", Todo),
    b("expand", "Раскрыть", Todo),
    b("coeffs", "Коэффициенты", Todo),
    b("collect", "Привести подобные", Todo),
    b("series", "Ряд", Todo),
    b("parfrac", "Простые дроби", Todo),
    b("fourier", "Преобразование Фурье", Todo),
    b("laplace", "Преобразование Лапласа", Todo),
    b("ztrans", "Z-преобразование", Todo),
];

pub fn buttons(p: Palette) -> &'static [Btn] {
    match p {
        Palette::Calculator => CALCULATOR,
        Palette::Graph => GRAPH,
        Palette::Matrix => MATRIX,
        Palette::Evaluation => EVALUATION,
        Palette::Calculus => CALCULUS,
        Palette::Boolean => BOOLEAN,
        Palette::Programming => PROGRAMMING,
        Palette::Greek => GREEK,
        Palette::Symbolic => SYMBOLIC,
    }
}

/// Ctrl+G: Latin → Greek, using the Mathcad (Symbol font) mapping.
pub fn to_greek(c: char) -> Option<char> {
    let lower = "αβχδεφγηιϕκλμνοπθρστυϖωξψζ";
    let upper = "ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ";
    let pick = |set: &str, i: usize| set.chars().nth(i);
    if c.is_ascii_lowercase() {
        pick(lower, (c as u8 - b'a') as usize)
    } else if c.is_ascii_uppercase() {
        pick(upper, (c as u8 - b'A') as usize)
    } else {
        // reverse mapping (Ctrl+G on a Greek letter gives the Latin one back)
        lower
            .chars()
            .position(|g| g == c)
            .map(|i| (b'a' + i as u8) as char)
            .or_else(|| upper.chars().position(|g| g == c).map(|i| (b'A' + i as u8) as char))
    }
}

/// Insert Function dialog (Ctrl+E): (category, [(name, template, description)]).
pub static FUNCTIONS: &[(&str, &[(&str, &str, &str)])] = &[
    (
        "Тригонометрические",
        &[
            ("sin", "sin(■)", "sin(z) — синус. Аргумент в радианах (или с единицей deg)."),
            ("cos", "cos(■)", "cos(z) — косинус."),
            ("tan", "tan(■)", "tan(z) — тангенс."),
            ("cot", "cot(■)", "cot(z) — котангенс."),
            ("sec", "sec(■)", "sec(z) — секанс."),
            ("csc", "csc(■)", "csc(z) — косеканс."),
            ("asin", "asin(■)", "asin(z) — арксинус, результат в радианах."),
            ("acos", "acos(■)", "acos(z) — арккосинус."),
            ("atan", "atan(■)", "atan(z) — арктангенс."),
            ("atan2", "atan2(■, ■)", "atan2(x, y) — угол точки (x, y), от −π до π."),
        ],
    ),
    (
        "Гиперболические",
        &[
            ("sinh", "sinh(■)", "sinh(z) — гиперболический синус."),
            ("cosh", "cosh(■)", "cosh(z) — гиперболический косинус."),
            ("tanh", "tanh(■)", "tanh(z) — гиперболический тангенс."),
            ("asinh", "asinh(■)", "asinh(z) — обратный гиперболический синус."),
            ("acosh", "acosh(■)", "acosh(z) — обратный гиперболический косинус."),
            ("atanh", "atanh(■)", "atanh(z) — обратный гиперболический тангенс."),
        ],
    ),
    (
        "Логарифмы и экспонента",
        &[
            ("exp", "exp(■)", "exp(z) — e в степени z."),
            ("ln", "ln(■)", "ln(z) — натуральный логарифм."),
            ("log", "log(■)", "log(z) — десятичный логарифм; log(z, b) — по основанию b."),
        ],
    ),
    (
        "Комплексные числа",
        &[
            ("Re", "Re(■)", "Re(z) — действительная часть."),
            ("Im", "Im(■)", "Im(z) — мнимая часть."),
            ("arg", "arg(■)", "arg(z) — аргумент (угол) комплексного числа."),
            ("conj", "conj(■)", "conj(z) — комплексно-сопряжённое."),
            ("csgn", "signum(■)", "signum(z) — z/|z|."),
        ],
    ),
    (
        "Округление и остаток",
        &[
            ("floor", "floor(■)", "floor(x) — наибольшее целое ≤ x."),
            ("ceil", "ceil(■)", "ceil(x) — наименьшее целое ≥ x."),
            ("round", "round(■, ■)", "round(x, n) — округление до n знаков после запятой."),
            ("trunc", "trunc(■)", "trunc(x) — целая часть."),
            ("mod", "mod(■, ■)", "mod(x, y) — остаток от деления x на y."),
            ("sign", "sign(■)", "sign(x) — знак числа: 1, 0 или −1."),
        ],
    ),
    (
        "Векторы и матрицы",
        &[
            ("augment", "augment(■, ■)", "augment(A, B) — объединение матриц по горизонтали."),
            ("stack", "stack(■, ■)", "stack(A, B) — объединение матриц по вертикали."),
            ("cols", "cols(■)", "cols(A) — число столбцов."),
            ("rows", "rows(■)", "rows(A) — число строк."),
            ("length", "length(■)", "length(v) — число элементов вектора."),
            ("last", "last(■)", "last(v) — индекс последнего элемента."),
            ("identity", "identity(■)", "identity(n) — единичная матрица n×n."),
            ("matrix", "matrix(■, ■, ■)", "matrix(m, n, f) — матрица из значений f(i, j)."),
            ("submatrix", "submatrix(■, ■, ■, ■, ■)", "submatrix(A, ir, jr, ic, jc) — подматрица."),
            ("transpose", "transpose(■)", "transpose(A) — транспонирование."),
            ("det", "det(■)", "det(A) — определитель."),
            ("tr", "tr(■)", "tr(A) — след матрицы."),
            ("lsolve", "lsolve(■, ■)", "lsolve(M, v) — решение системы M·x = v."),
            ("cross", "cross(■, ■)", "cross(a, b) — векторное произведение."),
            ("norme", "norme(■)", "norme(A) — евклидова норма."),
            ("sort", "sort(■)", "sort(v) — сортировка по возрастанию."),
            ("reverse", "reverse(■)", "reverse(v) — обратный порядок элементов."),
        ],
    ),
    (
        "Статистика",
        &[
            ("max", "max(■)", "max(A, B, …) — наибольшее значение."),
            ("min", "min(■)", "min(A, B, …) — наименьшее значение."),
            ("mean", "mean(■)", "mean(A) — среднее арифметическое."),
            ("median", "median(■)", "median(A) — медиана."),
            ("stdev", "stdev(■)", "stdev(A) — стандартное отклонение (генеральное)."),
            ("Stdev", "Stdev(■)", "Stdev(A) — стандартное отклонение (выборочное)."),
            ("var", "var(■)", "var(A) — дисперсия (генеральная)."),
            ("Var", "Var(■)", "Var(A) — дисперсия (выборочная)."),
            ("dnorm", "dnorm(■, ■, ■)", "dnorm(x, μ, σ) — плотность нормального распределения."),
            ("pnorm", "pnorm(■, ■, ■)", "pnorm(x, μ, σ) — функция нормального распределения."),
        ],
    ),
    (
        "Решение уравнений",
        &[
            ("root", "root(■, ■)", "root(f(x), x) — корень от начального значения x;\nroot(f(x), x, a, b) — корень на отрезке [a, b]."),
            ("polyroots", "polyroots(■)", "polyroots(v) — все корни полинома, коэффициенты по возрастанию степени."),
        ],
    ),
    (
        "Интерполяция",
        &[("linterp", "linterp(■, ■, ■)", "linterp(vx, vy, x) — линейная интерполяция.")],
    ),
    (
        "Мат. анализ",
        &[
            ("integrate", "integrate(■, ■, ■, ■)", "integrate(f(x), x, a, b) — определённый интеграл (пределы могут быть ∞)."),
            ("diff", "diff(■, ■)", "diff(f(x), x) — производная в текущей точке x;\ndiff(f(x), x, x0) — в точке x0."),
            ("sum", "sum(■, ■, ■, ■)", "sum(k, a, b, f(k)) — сумма; sum(i, f(i)) — по ранжированной переменной."),
            ("prod", "prod(■, ■, ■, ■)", "prod(k, a, b, f(k)) — произведение."),
        ],
    ),
    (
        "Специальные",
        &[
            ("Γ", "Γ(■)", "Γ(x) — гамма-функция."),
            ("erf", "erf(■)", "erf(x) — функция ошибок."),
            ("erfc", "erfc(■)", "erfc(x) — дополнительная функция ошибок."),
        ],
    ),
    (
        "Логические",
        &[("if", "if(■, ■, ■)", "if(cond, a, b) — a, если условие истинно, иначе b.")],
    ),
    (
        "Строки",
        &[
            ("concat", "concat(■, ■)", "concat(S1, S2, …) — объединение строк."),
            ("num2str", "num2str(■)", "num2str(z) — число в строку."),
            ("strlen", "strlen(■)", "strlen(S) — длина строки."),
        ],
    ),
    (
        "Единицы",
        &[("UnitsOf", "UnitsOf(■)", "UnitsOf(x) — единицы измерения величины x.")],
    ),
];

/// Insert Unit dialog (Ctrl+U): (dimension, units).
pub static UNITS: &[(&str, &[&str])] = &[
    ("Длина", &["m", "km", "dm", "cm", "mm", "μm", "nm", "in", "ft", "yd", "mi"]),
    ("Масса", &["kg", "gm", "mg", "tonne", "lb"]),
    ("Время", &["s", "ms", "μs", "ns", "min", "hr", "day", "week", "yr"]),
    ("Сила", &["N", "kN", "MN", "kgf", "tonnef", "dyne", "lbf", "kip"]),
    ("Давление", &["Pa", "kPa", "MPa", "GPa", "bar", "atm", "mmHg", "torr", "psi", "ksi"]),
    ("Энергия", &["J", "kJ", "MJ", "cal", "kcal", "Wh", "kWh", "eV", "BTU"]),
    ("Мощность", &["W", "mW", "kW", "MW", "hp"]),
    ("Частота", &["Hz", "kHz", "MHz", "GHz", "rpm"]),
    ("Скорость", &["kph", "mph"]),
    ("Площадь, объём", &["ha", "L", "mL", "liter", "gal"]),
    ("Ток, заряд", &["A", "mA", "kA", "C"]),
    ("Напряжение", &["V", "mV", "kV"]),
    ("Сопротивление", &["Ω", "kΩ", "MΩ", "S"]),
    ("Ёмкость, индуктивность", &["F", "mF", "μF", "nF", "pF", "H", "mH"]),
    ("Магнитные", &["Wb", "T"]),
    ("Температура", &["K", "Δ°C"]),
    ("Количество вещества", &["mol", "cd"]),
    ("Угол", &["rad", "deg", "°"]),
    ("Безразмерные", &["%"]),
];
