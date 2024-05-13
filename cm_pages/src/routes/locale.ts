const CD_LANGS = {
    'ar': 'ar_ae',
    'ar-AE': 'ar_ae',
    'cs': 'cs_cz',
    'cs-CZ': 'cs_cz',
    'de': 'de_de',
    'de-DE': 'de_de',
    'el': 'el_gr',
    'el-GR': 'el_gr',
    'en': 'default',
    'en-EN': 'default',
    'en-AU': 'en_au',
    'en-GB': 'en_gb',
    'en-PH': 'en_ph',
    'en-SG': 'en_sg',
    'es': 'es_es',
    'es-AR': 'es_ar',
    'es-ES': 'es_es',
    'es-MX': 'es_mx',
    'fr': 'fr_fr',
    'fr-FR': 'fr_fr',
    'hu': 'hu_hu',
    'hu-HU': 'hu_hu',
    'it': 'it_it',
    'it-IT': 'it_it',
    'ja': 'ja_jp',
    'ja-JP': 'ja_jp',
    'ko': 'ko_kr',
    'ko-KR': 'ko_kr',
    'pl': 'pl_pl',
    'pl-PL': 'pl_pl',
    'pt': 'pt_br',
    'pt-BR': 'pt_br',
    'ro': 'ro_ro',
    'ro-RO': 'ro_ro',
    'ru': 'ru_ru',
    'ru-RU': 'ru_ru',
    'th': 'th_th',
    'th-TH': 'th_th',
    'tr': 'tr_tr',
    'tr-TR': 'tr_tr',
    'vi': 'vi_vn',
    'vi-VN': 'vi_vn',
    'zh': 'zh_tw',
    'zh-CN': 'zh_cn',
    'zh-MY': 'zh_my',
    'zh-TW': 'zh_tw',
};

export function getCdLang(): string {
    const langs = (null != navigator.languages ? navigator.languages : [navigator.language]);
    for (const lang of langs) {
        if (null != CD_LANGS[lang as keyof typeof CD_LANGS]) {
            return CD_LANGS[lang as keyof typeof CD_LANGS];
        }
    }
    return 'default';
}
